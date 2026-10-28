# M1.4 Store, watcher, tree, palette and keys: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The collection on disk and the TUI stay in step: edits made elsewhere show within a second, a file that stops parsing becomes an error node, and the tree can create, rename, delete and reorder requests and folders, with atomic writes whose `git diff` shows only the lines that changed. Every key comes from one table that `~/.config/pawsman/keys.toml` can override, and Ctrl-K opens a fuzzy palette over requests and actions.

**Architecture:**

- `pawsman-model` stays pure:
  - Request ids become stable across rebuilds through an `Ids` map from path to id.
  - Folders are also found through their `_folder.toml`, so an empty folder shows.
  - A new `edit` module changes single keys (`name`, `order`) with `toml_edit`, so comments and layout survive.
- `pawsman-core`:
  - `store::Files` caches the bytes of the collection files and refreshes the paths it is told about.
  - `store::write` writes atomically.
  - `tree::plan` turns a tree operation into a list of file changes (pure); `store::apply` runs them.
  - `watch::Watcher` wraps `notify` with its own debounce.
  - The engine thread ties these together. It reloads on watcher batches, runs operations, ignores its own writes because their bytes are already cached, and remembers the active environment in `.pawsman/state.toml`.
  - `keys::Keymap` replaces the static table and reads overrides; `fuzzy` ranks palette items.
- `pawsman-tui` gains:
  - tree navigation with expand and collapse;
  - a one-line prompt for names;
  - a yes/no dialog;
  - the palette.
- `pawsman-cli` gains `pawsman init [DIR]` and loads `keys.toml`.

**Tech stack:** notify 8.2.1 (new), toml_edit 0.25.17 (new). Everything from 1.1–1.3 stays.

**Spec:** [M1 design, piece 1.4](../specs/2026-10-26-m1-http-client-design.md#14-store-watcher-tree-palette-and-keys), with [program spec §2](../specs/2026-10-23-pawsman-design.md#2-on-disk-format-variables-and-secrets) (layout, `order`) and [§3 Keys](../specs/2026-10-23-pawsman-design.md#keys).

## Global constraints

- Everything in the [1.1](2026-10-26-m1-1-binary-terminal-splash.md#global-constraints), [1.2](2026-10-26-m1-2-thin-path.md#global-constraints) and [1.3](2026-10-27-m1-3-format-envs-interpolation.md#global-constraints) constraints still holds.
- Files are the source of truth (rules/rust-api-client.md §8). The store writes atomically (a temp file in the same directory, then rename). The watcher reloads external edits, and an unparsable file shows as an error node, never a crash.
- `pawsman-model` does no I/O. `tree::plan` and `fuzzy` are pure; only `store` and `watch` touch the file system.
- A write never leaves the collection directory, never overwrites a file it did not mean to (`core::exists`), and keeps the target's permissions. A new file gets `0644` on Unix.
- Renaming never reorders: `order` entries follow the rename (program spec §2).
- Typed characters are never commands while a text field (prompt or palette query) has focus (rules/tui.md §5).
- Ctrl-Q quits by default. `keys.toml` may rebind it but can never leave quit or help without a key.
- Dependencies are vetted (rules §1) and pinned with `=`:
  - **notify 8.2.1:** CC0-1.0, released 2026-10-25, 182 M downloads, 3.5 k stars, not archived.
  - **toml_edit 0.25.17:** MIT OR Apache-2.0, released 2026-10-09, 254 M recent downloads. It is the toml-rs family and shares `toml_parser` and `toml_writer` with the `toml` crate already in the tree.
- **Spec deviations, each with a DECISIONS line:**
  - `nucleo-matcher` is replaced by an own matcher. It is MPL-2.0, which the `deny.toml` licence allow-list does not include, and widening that list is the user's call. Its last release was 2024-02. The alternative, frizbee (MIT, active), holds about 900 `unsafe` SIMD blocks, which is more than a palette over hundreds of items needs. The own scorer is about 120 lines, pure and property-tested.
  - `notify-debouncer-full` is replaced by a 30-line debounce. Its rename matching by file id is not needed: pawsman reloads by path, and it would add walkdir, file-id and notify-types.
  - `toml_edit` is added, so a rename or reorder changes only its own line.

## Review focus

1. **An editor that saves through a temp file and a rename (vim, VS Code), or a `git checkout` that rewrites many files at once.** One reload, with the final contents and no error flash from a half-written file:
   - Task 5 `a_burst_of_writes_is_one_batch` and `a_rename_over_the_file_is_reported`;
   - Task 6 `a_burst_of_edits_reloads_the_final_content`.
2. **A rename or new name that slugs to something odd**: `../x`, `a/b`, `.hidden`, an empty name, an existing file's name, or `envs` at the root. The write never leaves the folder and never overwrites a file; it is `core::exists`, a renamed slug or a refusal:
   - Task 2 `slug_cases`;
   - Task 4 `rename_never_leaves_the_folder`, `rename_onto_an_existing_file_is_exists` and `a_root_folder_named_envs_gets_another_name`.
3. **Deleting a folder that also holds files that are not collection files** (`notes.md`, images). Only collection files go; the folder stays, with a notice (Task 4 `deleting_a_folder_keeps_other_files`, Task 6 `a_kept_folder_raises_a_notice`).
4. **A `keys.toml` that unbinds quit, binds a letter twice, or binds a letter a text field needs** (`send = ["a"]`):
   - quit and help keep a key: Task 7 `quit_and_help_cannot_be_unbound`;
   - a conflict is reported: Task 7 `a_conflict_is_keys_invalid_and_names_both`;
   - letters type into the prompt and the palette: Task 9 `letters_type_in_the_prompt_even_when_bound`, Task 10 `j_and_k_type_in_the_palette`.
5. **A `pawsman.toml` that breaks during a reload, or a file that is briefly unreadable.** The previous collection stays, a notice explains, and the next good save recovers (Task 6 `a_broken_manifest_keeps_the_collection_and_recovers`).

---

## File map

| File | Responsibility |
| --- | --- |
| `crates/pawsman-model/src/ids.rs` | `Ids`: stable request ids by path |
| `crates/pawsman-model/src/collection.rs` | `build` with `Ids`, folders from `_folder.toml`, id index |
| `crates/pawsman-model/src/edit.rs` | `toml_edit` changes: `set_name`, `set_order`, order entries, new files, `slug` |
| `crates/pawsman-core/src/store.rs` | `Files` (walk, refresh, cache), `write`, `apply`, `init` |
| `crates/pawsman-core/src/tree.rs` | `NodeRef`, `Op`, `Change`, `plan` |
| `crates/pawsman-core/src/watch.rs` | `Watcher`: notify, per-directory watches, debounce |
| `crates/pawsman-core/src/memory.rs` | `.pawsman/state.toml`: the active environment |
| `crates/pawsman-core/src/state.rs`, `engine.rs` | reloads, operations, created node, environment memory |
| `crates/pawsman-core/src/keys.rs`, `keys/chord.rs`, `keys/keymap.rs` | actions, contexts, `Keymap`, chord parsing, overrides |
| `crates/pawsman-core/src/fuzzy.rs` | palette scoring |
| `crates/pawsman-tui/src/tree.rs` | visible rows, collapsed folders, cursor moves |
| `crates/pawsman-tui/src/views/{sidebar,request,prompt,ask,palette,help,hints}.rs`, `app.rs` | drawing and key handling |
| `crates/pawsman-cli/src/{args,main}.rs` | `init`, `keys.toml` |
| `crates/pawsman-cli/tests/it/{tree,init,keys}.rs` | end-to-end checks |

---

### Task 1: model — stable ids, folders from `_folder.toml`, entry index

**Files:** create `crates/pawsman-model/src/ids.rs`; modify `collection.rs`, `lib.rs`.

**Interfaces — produces:**

- `Ids` (`#[derive(Debug, Clone, Default)]`):
  - `Ids::id(&mut self, path: &Path) -> RequestId` returns the path's id, or the next unused one.
  - `Ids::rename(&mut self, from: &Path, to: &Path)` moves `from`, and every path under `from` when it is a directory, to the same place under `to`.
  - `Ids::retain(&mut self, live: impl Fn(&Path) -> bool)` drops the paths that are gone. Numbers are never reused within a session.
- `Collection::build(root: &Path, files: Vec<SourceFile>, ids: &mut Ids) -> Result<Collection, ModelError>`. `from_files` stays and calls it with a fresh `Ids`, so ids in a fresh build are 1…n in tree order, as before.
- Folders are discovered from the `*.req.toml` paths and also from `_folder.toml` paths, so a folder that holds only `_folder.toml` is a node.
- `Collection::entry(id)` looks up through a `HashMap<RequestId, usize>` index (#52: the sidebar was O(n²)).
- `Entry::folder(&self) -> &Path` (the folder path, already stored) and `Entry::stem(&self) -> &str` (the file name without `.req.toml`) become public.

- [ ] **Step 1: failing tests** in `collection.rs`, plus `ids.rs` unit tests:
  - `ids_survive_a_rebuild_with_a_file_sorted_before`: build with `b.req.toml`, then rebuild with `a.req.toml` added through the same `Ids`. `b` keeps id 1 and `a` gets 2.
  - `ids_follow_a_folder_rename`: `users/x.req.toml` is id 1; after `ids.rename("users", "people")`, a rebuild with `people/x.req.toml` gives id 1.
  - `a_folder_with_only_a_folder_file_is_a_node`: `empty/_folder.toml` with `name = "Empty"` gives a `Node::Folder` named `Empty` with no children.
  - `retain_forgets_removed_paths`.
  - `stems_and_folders_are_exposed`.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-model`. Expected: FAIL (no `Ids`, no `build`). Commit `test: stable request ids and empty folders`.
- [ ] **Step 3: implement.**
  - `Builder` holds `&mut Ids` and calls `ids.id(path)` instead of counting.
  - `children()` also scans the `_folder.toml` keys for subfolders.
  - Build `index` after the tree.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Every 1.3 test still passes, since fresh builds number in tree order. Commit `feat: stable request ids and empty folders`.

### Task 2: model — single-key edits with `toml_edit`

**Files:** create `crates/pawsman-model/src/edit.rs`; modify `lib.rs`, `Cargo.toml` (`toml_edit`), root `Cargo.toml` (`toml_edit = { version = "=0.25.17", default-features = false, features = ["parse", "display"] }`).

**Interfaces — produces** (all pure; `path` only names the file in errors):

- `edit::set_name(text: &str, path: &Path, name: &str) -> Result<String, ModelError>` sets the top-level `name`. Every other byte stays, comments included. An empty `text` gives a file with just `name`.
- `edit::set_order(text: &str, path: &Path, order: &[String]) -> Result<String, ModelError>` writes `order = [...]` as one inline array. It replaces the key in place, or adds it after `name` (else at the top).
- `edit::rename_in_order(text, path, from: &str, to: &str) -> Result<Option<String>, ModelError>` returns `None` when `from` is not listed.
- `edit::remove_from_order(text, path, key: &str) -> Result<Option<String>, ModelError>`.
- `edit::new_request(name: &str) -> String` is `name = "…"\nmethod = "GET"\nurl = ""\n`, with the name escaped by `toml_edit`.
- `edit::new_folder(name: &str) -> String` and `edit::new_manifest(name: &str) -> String` (`format = 1\nname = "…"\n`).
- `edit::slug(name: &str) -> String`:
  - lowercases;
  - keeps Unicode alphanumerics, `_` and `-`;
  - turns any other run into one `-`;
  - trims `-` at both ends;
  - caps the result at 64 characters.

  An empty result is `""`; callers substitute `request` or `folder`.
- Broken TOML is `model::invalid_file`.

- [ ] **Step 1: failing tests** in `edit.rs`:
  - `set_name_keeps_comments_and_layout`: a file with a comment, a blank line and `[headers]`. Only the `name` line differs, compared line by line.
  - `set_order_replaces_one_line`, and `set_order_adds_after_name`.
  - `rename_in_order_and_remove_from_order`, including the `None` cases.
  - `new_files_parse`: `Request::parse(&new_request("A \"q\" ü"), …)` has that name. `Manifest::parse(new_manifest(…))` works, and so does `FolderDefaults` through a `Collection` built from `new_folder`.
  - `slug_cases` (rstest):
    - `"Create user"` → `create-user`
    - `"../../etc"` → `etc`
    - `"a/b"` → `a-b`
    - `".hidden"` → `hidden`
    - `"  "` → `""`
    - `"Пользователи"` → `пользователи`
    - `"x".repeat(100)` → 64 × `x`
  - `broken_toml_is_invalid_file`.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-model edit`. Expected: FAIL. Commit `test: single-key toml edits`.
- [ ] **Step 3: implement** with `toml_edit::DocumentMut`. Keep each key's decor (prefix comments) on replace.
- [ ] **Step 4: run** `cargo nextest run -p pawsman-model`, `cargo deny check`, `tools/check-deps.sh`. Expected: PASS. Commit `feat: single-key toml edits`.

### Task 3: core — the file cache, atomic writes and `init`

**Files:** rewrite `crates/pawsman-core/src/store.rs`; modify `error.rs`, `codes.rs`, `Cargo.toml` (`tempfile` moves from dev to normal); create `docs/errors/core/{write,read,exists}.md`.

**Interfaces — produces:**

- `store::Files`:
  - `Files::read(dir: &Path) -> Files` walks with the 1.3 rules (hidden dirs, `node_modules`, `target`, symlinks, depth 16, 10 000 files).
  - `Files::root(&self) -> &Path`.
  - `Files::dirs(&self) -> &BTreeSet<PathBuf>`: every directory walked, the root as `""`.
  - `Files::get(&self, rel: &Path) -> Option<&[u8]>` and `Files::paths(&self) -> impl Iterator<Item = &Path>`.
  - `Files::refresh(&mut self, rel: &[PathBuf]) -> Refresh`. For each path:
    - a directory now present: walk it, replacing everything under it;
    - a wanted file: read it;
    - a path that is gone: drop it and everything under it.

    Paths outside the walk rules are ignored.
  - `Refresh { changed: bool, errors: Vec<CoreError> }`. `changed` is false when every byte is the same as before. Unreadable files are `core::read` errors, and the old bytes stay.
  - `Files::put(&mut self, rel, bytes)` and `Files::forget(&mut self, rel)`, for the engine's own writes.
  - `Files::collection(&self, ids: &mut Ids) -> Result<Collection, ModelError>`.
- `store::relevant(rel: &Path) -> bool` is the shared filter for the walk and the watcher. A path is relevant when no component is hidden (starts with `.`), except a top-level `.env`. A path is not relevant when a component is `node_modules` or `target`. The watcher also uses it to drop editor temp files (`.x.swp`, `.tmpAbC`).
- `store::load(dir) -> Result<Collection, ModelError>` stays as `Files::read(dir).collection(&mut Ids::default())`, with the 1.3 log line.
- `store::write(root: &Path, rel: &Path, bytes: &[u8]) -> Result<(), CoreError>`:
  1. Create missing parent directories.
  2. Write a `tempfile::NamedTempFile::new_in(parent)` and call `sync_all`.
  3. Set its permissions to the target's, or to `0o644` on Unix for a new file.
  4. `persist` it.

  Failure is `core::write` (retryable) and names the path.
- `store::init(dir: &Path) -> Result<(), CoreError>`:
  - Creates `dir` if missing.
  - Writes `pawsman.toml` (`edit::new_manifest`, named after the directory's file name). If `pawsman.toml` already exists, it is `core::exists`.
  - Creates `.gitignore`, or appends the missing lines of `.env` and `.pawsman/`.
- New codes:
  - `core::write`: retryable, "`{path}` cannot be written: {reason}";
  - `core::read`: retryable;
  - `core::exists`: permanent, "`{path}` already exists".

  Each code goes into `ALL` and gets a page.

- [ ] **Step 1: failing tests** in `store.rs` (tempdir):
  - `refresh_reads_a_changed_file`, `refresh_drops_a_removed_file` and `refresh_walks_a_new_directory`;
  - `refresh_with_the_same_bytes_is_unchanged`;
  - `hidden_and_skipped_paths_are_not_relevant` (rstest over `.git/x.req.toml`, `.tmpX`, `a/.x.swp`, `node_modules/a.req.toml`, `.env` (relevant) and `users/a.req.toml` (relevant));
  - `write_replaces_atomically_and_keeps_permissions`: target `0o640`; after the write, the content is new, the mode is still `0o640` and no other file is left in the directory;
  - `write_creates_a_new_file_0644`;
  - `write_into_a_read_only_dir_is_core_write` (`#[cfg(unix)]`; skip when running as root);
  - `init_writes_manifest_and_gitignore`, `init_refuses_an_existing_collection` and `init_appends_to_an_existing_gitignore_once`;
  - the 1.3 walk tests stay.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-core store`. Expected: FAIL. Commit `test: file cache, atomic writes and init`.
- [ ] **Step 3: implement.** One walk function serves `read` and the directory case of `refresh`.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: file cache, atomic writes and init`.

### Task 4: core — tree operations as plans

**Files:** create `crates/pawsman-core/src/tree.rs`; modify `store.rs` (`apply`), `lib.rs`.

**Interfaces — produces:**

- `tree::NodeRef { Request(RequestId), Folder(PathBuf) }`: `Debug, Clone, PartialEq, Eq`. The folder path is relative, and the root is `""`.
- `tree::Direction { Up, Down }`.
- `tree::Op`, which is `NewRequest { parent: PathBuf, name: String }`, `NewFolder { parent: PathBuf, name: String }`, `Rename { node: NodeRef, name: String }`, `Delete(NodeRef)` or `Move { node: NodeRef, dir: Direction }`.
- `tree::Change`, which is `Write { path: PathBuf, bytes: Vec<u8> }`, `Rename { from: PathBuf, to: PathBuf }`, `Remove(PathBuf)` or `RemoveDirIfEmpty(PathBuf)`. All paths are relative.
- `tree::Plan { changes: Vec<Change>, select: Option<PathBuf> }`. `select` is the path of the node to select afterwards: the new or renamed request file, or the folder directory.
- `tree::plan(collection: &Collection, files: &Files, op: &Op) -> Result<Plan, CoreError>`. It is pure and reads texts from `files`. Its rules:
  - **Keys:** a child's key in `order` is its directory name (folder) or its stem (request).
  - **Unique names:** `slug(name)`, else `request` or `folder`, then `-2`, `-3`, … until neither `<key>.req.toml` nor a directory `<key>` exists in the parent. At the root, `envs` counts as taken.
  - **NewRequest:** `Write(<parent>/<key>.req.toml, new_request(name))`.
  - **NewFolder:** `Write(<parent>/<key>/_folder.toml, new_folder(name))`.
  - **Rename request:** if `slug(name)` differs from the stem, `Rename(old, <parent>/<slug>.req.toml)`, which is `core::exists` when that file exists. Then `Write(<new path>, set_name(text, name))`, and the parent's `_folder.toml` through `rename_in_order` when it lists the stem. A file that does not parse fails with `model::invalid_file`, wrapped as a notice by the engine.
  - **Rename folder:** `Rename` of the directory when the slug differs (`core::exists` if it is taken). Then `Write(<dir>/_folder.toml, set_name(text or "", name))`, and the parent's order entry.
  - **Delete request:** `Remove` of the `.req.toml` file and of every cached `<stem>.body.*` in the same folder, plus `remove_from_order` in the parent.
  - **Delete folder:** `Remove` of every cached file under it, then `RemoveDirIfEmpty` for each cached directory under it, deepest first, then the folder itself, plus the parent's order entry.
  - **Move:** take the parent's children keys in display order and swap the node with its neighbour. Then `Write(<parent>/_folder.toml, set_order(text or "", keys))`. The first child moving up, or the last moving down, is an empty plan.
  - An unknown id or folder is an empty plan.
- `store::apply(root: &Path, changes: &[Change]) -> Result<Vec<PathBuf>, CoreError>` runs the changes in order. A `Rename` onto an existing path is `core::exists`. A `RemoveDirIfEmpty` that finds files left is no error. It returns the directories that stayed non-empty, for the notice.

- [ ] **Step 1: failing tests** in `tree.rs`, with `Files` built in memory through a test helper `Files::from_pairs(root, &[(&str, &str)])` (`#[cfg(test)]`):
  - `new_request_takes_a_free_slug`: `Create user` → `users/create-user.req.toml`, and `-2` when taken;
  - `a_root_folder_named_envs_gets_another_name`;
  - `rename_moves_the_file_and_its_order_entry`: `order = ["b", "a"]`, renaming `a` to `Zed` gives `Rename(a → zed)`, the new name in the file, and `order = ["b", "zed"]`;
  - `rename_with_the_same_slug_only_sets_the_name`;
  - `rename_onto_an_existing_file_is_exists`;
  - `rename_never_leaves_the_folder`: `../../etc` stays in the same folder as `etc.req.toml`;
  - `deleting_a_request_removes_its_sidecars`;
  - `deleting_a_folder_keeps_other_files`: the plan has no `Remove` for `notes.md`;
  - `move_up_writes_the_full_order` and `move_at_the_edge_is_empty`;
  - `move_in_the_root_writes_root_folder_file`;
  - plus, in `store.rs`, `apply_runs_changes_and_reports_kept_dirs` against a tempdir.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-core tree store`. Expected: FAIL. Commit `test: tree operations as plans`.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `cargo nextest run -p pawsman-core`. Expected: PASS. Commit `feat: tree operations as plans`.

### Task 5: core — the watcher

**Files:** create `crates/pawsman-core/src/watch.rs`, `docs/errors/core/watch.md`; modify `codes.rs`, `error.rs`, `Cargo.toml` (`notify`), root `Cargo.toml` (`notify = { version = "=8.2.1", default-features = false, features = ["macos_fsevent"] }`), `deny.toml` (a wrapper row: `notify` only from `pawsman-core`), `tools/check-deps.sh` (forbid `notify` in `pawsman-model` and `pawsman-net`).

**Interfaces — produces:**

- `watch::Watcher::start(root: &Path, sink: impl Fn(Vec<PathBuf>) + Send + 'static) -> Result<Watcher, CoreError>`.
  - It starts `notify::recommended_watcher` on a channel and a `pawsman-watch` thread.
  - The thread debounces: after the first event, it collects until 100 ms pass without one, or 1 s in all. It then calls `sink` once with the relative, deduplicated, sorted paths that `store::relevant` keeps. An empty batch is not sent.
  - Failure is `core::watch` (permanent).
- `Watcher::sync(&mut self, dirs: &BTreeSet<PathBuf>)` watches each directory non-recursively and unwatches the ones that are gone. A failed `watch` call is logged and skipped; the next sync tries again.
- Dropping the `Watcher` stops notify. The thread ends when its channel closes.
- Logs: `debug!(paths, "files changed")` per batch, and `warn!(dir, %err, "watch failed")`.

- [ ] **Step 1: failing tests** in `watch.rs` (tempdir, real notify, the sink sends into an `mpsc` channel, `recv_timeout(2 s)`):
  - `a_write_is_reported`;
  - `a_burst_of_writes_is_one_batch`: 20 writes 5 ms apart give exactly one batch within 2 s, and nothing more within 300 ms after it;
  - `a_rename_over_the_file_is_reported`: write `.a.req.toml.tmp`, then rename it onto `a.req.toml`. The batch has `a.req.toml` and not the hidden temp file;
  - `a_new_subdirectory_is_watched_after_sync`: create `users/`, sync, then write `users/x.req.toml`; it is reported;
  - `hidden_paths_are_not_reported`: a write to `.git/HEAD`, then to `a.req.toml`. The first batch holds only `a.req.toml`.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-core watch`. Expected: FAIL. Commit `test: file watcher`.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `cargo nextest run -p pawsman-core`, `cargo deny check`, `tools/check-deps.sh`. Expected: PASS. Commit `feat: file watcher`.

### Task 6: core — reloads, operations and the remembered environment

**Files:** create `crates/pawsman-core/src/memory.rs`; modify `state.rs`, `engine.rs`, `lib.rs`; tests in `crates/pawsman-core/tests/it/{engine,store}.rs`.

**Interfaces — produces:**

- `memory::load(root: &Path) -> Option<String>` reads `.pawsman/state.toml` (`env = "…"`). A missing or broken file is `None`, with a `debug!`.
- `memory::save(root: &Path, env: Option<&str>) -> Result<(), CoreError>` goes through `store::write` (`env` is omitted for none).
- In `state`:
  - `Event::Reloaded(Result<Collection, ModelError>)`:
    - `Ok` replaces the collection. Statuses of ids no longer present are dropped. When the active environment is gone or broken, it is cleared with a notice. Collection issues that are new become notices.
    - `Err` keeps the previous collection and raises the error as a notice.
  - `Event::Created(PathBuf)` is set by the engine after an operation.
  - `Snapshot::created(&self) -> Option<&Path>` is delivered once, like notices. The UI maps it to a node.
- In `engine`:
  - `Engine::apply(&self, op: tree::Op)` and `Engine::reload(&self)`.
  - The engine thread owns `Files`, `Ids` and an `Option<Watcher>`.
  - **At start:** `Files::read`, `collection(&mut ids)`, then `Watcher::start`. A failure raises a `core::watch` notice, and Ctrl-R still reloads. Then `watcher.sync(files.dirs())`, then `memory::load` → select that environment if it is valid, silently.
  - **`Msg::Changed(paths)` from the watcher:**
    1. `files.refresh(&paths)`; each error becomes a notice.
    2. If changed, `Event::Reloaded(files.collection(&mut ids))`.
    3. `watcher.sync(files.dirs())`.
    4. `ids.retain(|p| files.get(p).is_some())`.
  - **`Msg::Op(op)`:**
    1. `tree::plan`, then `store::apply`.
    2. For every `Rename`, `ids.rename(from, to)`.
    3. `files.refresh(touched paths)` right away, so the watcher's later batch finds the same bytes and changes nothing (own writes are ignored).
    4. Reload, `Event::Created(plan.select)`, and a notice for each kept directory ("`notes/` kept: it holds other files").

    Errors become notices.
  - **`Msg::Reload`:** `Files::read` again, then `Reloaded`.
  - **`Event::SelectEnv`:** also `memory::save`; a failure is a `core::write` notice.
- Logs: `info!(paths, ms, "collection reloaded")`, `info!(op = ?kind, "tree changed")` (names, not contents), and `warn!` per refresh error.

- [ ] **Step 1: failing tests:**
  - `state.rs`: `reload_keeps_statuses_of_surviving_ids_and_drops_the_rest`, `reload_clears_an_env_that_broke`, `a_failed_reload_keeps_the_collection_with_a_notice`, `created_is_delivered_once`.
  - `tests/it/engine.rs`, each waiting on snapshots with a 2 s deadline:
    - `an_external_edit_reaches_a_snapshot_within_a_second` (it asserts under 1 s from the write);
    - `a_file_that_stops_parsing_becomes_an_error_entry`;
    - `a_burst_of_edits_reloads_the_final_content`: 30 writes, and the final snapshot holds the last name;
    - `new_request_creates_the_file_and_reports_it_created`;
    - `rename_keeps_the_id_and_the_response`: send to `FakeHttp`, rename, and the status is still `Done` under the same id;
    - `own_writes_do_not_reload_twice`: count the snapshots after a rename within 500 ms. Exactly one carries a new generation from the reload, plus none from the watcher echo;
    - `a_kept_folder_raises_a_notice`;
    - `a_broken_manifest_keeps_the_collection_and_recovers`;
    - `the_active_env_is_remembered_across_restarts`.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-core`. Expected: FAIL. Commit `test: reloads, tree operations and env memory in the engine`.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: reloads, tree operations and env memory in the engine`.

### Task 7: core — the keymap and `keys.toml`

**Files:** create `crates/pawsman-core/src/keys/keymap.rs`, `docs/errors/core/keys_invalid.md`; modify `keys.rs`, `keys/chord.rs` (`FromStr`), `paths.rs` (`keys_file()`), `codes.rs`, `error.rs`; adapt the callers in `pawsman-tui` (`app.rs`, `views/hints.rs`, `views/help.rs`, `options.rs`) so the build stays green.

**Interfaces — produces:**

- New actions, with default keys, contexts and whether they show in the bar:

  | Action | Name | Keys | Contexts | Bar |
  | --- | --- | --- | --- | --- |
  | `Palette` | `palette` | Ctrl-K | Main | yes, as "palette" |
  | `Reload` | `reload` | Ctrl-R | Main | help only |
  | `NewRequest` | `new_request` | `a` | Main | help only |
  | `NewFolder` | `new_folder` | `A` | Main | help only |
  | `Rename` | `rename` | `r`, F2 | Main | help only |
  | `Delete` | `delete` | `d`, Delete | Main | help only |
  | `MoveUp` | `move_up` | `K` | Main | help only |
  | `MoveDown` | `move_down` | `J` | Main | help only |
  | `Expand` | `expand` | Right, `l` | Main | help only |
  | `Collapse` | `collapse` | Left, `h` | Main | help only |

  Existing actions gain names: `quit`, `help`, `close`, `copy`, `send`, `cancel`, `next_pane`, `prev_pane`, `up`, `down`, `page_up`, `page_down`, `pick_env`, `trace`, `confirm`.
- New contexts: `Prompt` (a one-line text field), `Ask` (yes or no) and `Palette`.
  - `Confirm` (Enter) applies in `Main` (it toggles a folder), `Picker`, `Prompt`, `Ask` and `Palette`.
  - `Close` (Esc) applies in `Help`, `Picker`, `Prompt`, `Ask` and `Palette`.
  - `Up` and `Down` apply in `Palette` through the arrow keys only. Letters never reach lookup in text contexts (Tasks 9 and 10).
  - `y` is `Confirm` and `n` is `Close` in `Ask`.
- `Action::name(self) -> &'static str` and `impl FromStr for Action`.
- `impl FromStr for Chord`, the inverse of `Display`:
  - modifiers `Ctrl-`, `Alt-`, `Shift-`, `Super-`, `Hyper-`, `Meta-`;
  - named keys as `Display` prints them (`Enter`, `Esc`, `Tab`, `BackTab`, `Backspace`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`, `Insert`, `Delete`, `F1`–`F24`);
  - any single character.

  Ctrl and Alt letters are lowercased.
- `keys::Keymap`:
  - `Keymap::default()` holds the table;
  - `bindings(&self) -> &[Binding]`;
  - `lookup(&self, Context, Chord) -> Option<Action>`, with layout folding as before;
  - `hints(&self, Context) -> Vec<(String, &'static str)>`;
  - `label(&self, Action) -> Option<String>`, the first key;
  - `Keymap::parse(text: &str, path: &Path) -> Result<Keymap, CoreError>`;
  - `Keymap::load(path: &Path) -> (Keymap, Option<CoreError>)`: a missing file is the defaults with no error, and an invalid one is the defaults plus the error.

  `Binding.chords` becomes `Vec<Chord>`, and the free functions `keys::table`, `lookup` and `hints` go.
- The `keys.toml` format is `[keys]` with `<action name> = ["<chord>", …]`. Each listed action's keys are replaced in all of its contexts. Any one of these makes the whole file invalid (`core::keys_invalid`, permanent, "`{path}` is not valid: {reason}"), and the defaults apply:
  - an unknown action;
  - a bad chord;
  - an empty list for `quit` or `help`;
  - one chord on two actions that share a context, naming both actions;
  - an unknown top-level key.
- `Paths::keys_file()` is `config_dir/keys.toml`.
- TUI plumbing in this task:
  - `Options::keymap(Keymap)`;
  - `App` holds an `Arc<Keymap>` and calls `self.keys.lookup`;
  - `hints::fit` and `help::draw` take `&Keymap`;
  - `quit_label` reads `keymap.label(Action::Quit)`.

- [ ] **Step 1: failing tests** in `keys.rs` and `keymap.rs`:
  - `new_main_bindings` (rstest over the table above, with Cyrillic folding for `a` as `ф`);
  - `every_default_label_parses_back`: `chord.to_string().parse() == chord` for every default chord;
  - `chord_parse_errors`: `Ctrl-`, `Foo-K` and an empty string;
  - `overrides_replace_an_actions_keys`: `send = ["F6"]` makes F6 send and F5 do nothing;
  - `an_unknown_action_is_keys_invalid`;
  - `a_conflict_is_keys_invalid_and_names_both`: `trace = ["e"]` clashes with `pick_env`;
  - `quit_and_help_cannot_be_unbound`;
  - `a_missing_file_is_the_defaults`;
  - `main_hints_lead_with_send` becomes `["F5", "Tab", "Ctrl-K", "e", "t", "Esc", "Ctrl-Q", "F1"]`;
  - every existing `lookup` test moves onto `Keymap::default()` unchanged.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-core keys`. Expected: FAIL. Commit `test: keymap with keys.toml overrides`.
- [ ] **Step 3: implement**, then adapt the TUI callers.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS, with the help snapshot updated for the new rows. Review the snapshot diff, then commit `feat: keymap with keys.toml overrides`.

### Task 8: core — fuzzy matching

**Files:** create `crates/pawsman-core/src/fuzzy.rs`; modify `lib.rs`, `Cargo.toml` (dev: `proptest`).

**Interfaces — produces:**

- `fuzzy::Match { score: u32, positions: Vec<usize> }`. Positions are the char indices of the matched characters, read through `score()` and `positions()`.
- `fuzzy::score(query: &str, text: &str) -> Option<Match>`:
  - It is case-insensitive (`char::to_lowercase`, first char) and ignores spaces in the query.
  - Every query char must appear in order.
  - It tries each start position of the first char and keeps the best.
  - Per matched char:
    - base 16;
    - +24 at the start of the text;
    - +20 after a separator (space, `-`, `_`, `.`, `/`) or at a lower-to-upper case change;
    - +12 when it directly follows the previous match;
    - −1 per skipped char between matches, capped at −8 per gap.
  - An empty query matches with score 0 and no positions.
- `fuzzy::rank<T>(query: &str, items: impl IntoIterator<Item = (T, String)>) -> Vec<(T, Match)>` sorts by score descending, then by the original order.

- [ ] **Step 1: failing tests:**
  - `a_subsequence_is_required`;
  - `word_starts_beat_middles`: `cu` ranks `create user` above `accuracy`;
  - `consecutive_beats_scattered`: `user` ranks `users` above `u-s-e-r`;
  - `case_is_ignored`;
  - `cyrillic_matches`: `пол` matches `Пользователи`;
  - `empty_query_keeps_the_order`;
  - `positions_mark_the_matched_chars`;
  - proptest `positions_are_increasing_and_spell_the_query`, for any `text` and a query drawn as a subsequence of it.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-core fuzzy`. Expected: FAIL. Commit `test: fuzzy matching`.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run.** Expected: PASS. Commit `feat: fuzzy matching`.

### Task 9: TUI — tree navigation, the name prompt and the yes/no dialog

**Files:** create `crates/pawsman-tui/src/tree.rs`, `views/prompt.rs`, `views/ask.rs`; modify `app.rs`, `views/sidebar.rs`, `views/request.rs` (a folder summary), `views/main.rs`, `views.rs`.

**Interfaces — produces:**

- `tree::Row { node: NodeRef, depth: usize }` and `tree::rows(collection, collapsed: &BTreeSet<PathBuf>) -> Vec<Row>`, holding the visible rows in display order. A collapsed folder's row is shown and its children are not.
- `App` state:
  - `selected: Option<NodeRef>` replaces `Option<RequestId>`; the request pane gets the id when a request is selected;
  - `collapsed: BTreeSet<PathBuf>`.
- Keys in Main:
  - Up and Down move over rows, folders included.
  - `Collapse` collapses an open folder, or moves to the parent row.
  - `Expand` opens a folder.
  - `Confirm` toggles a folder.
  - The selection survives a reload by id or path. A vanished node selects the row at the same index, clamped.
- `Snapshot::created()` selects that path's node and expands its ancestors.
- Overlays:
  - `Overlay::Prompt(Prompt)`, where `Prompt { title: String, text: String, purpose: PromptFor }` and `PromptFor` is `NewRequest { parent }`, `NewFolder { parent }` or `Rename(NodeRef)`.
  - `Overlay::Ask(Ask)`, where `Ask { text: String, op: Op }`.
- Text entry runs before key lookup when the overlay is `Prompt` (and `Palette` in Task 10):
  - a chord whose mods are empty or Shift and whose code is `Char` inserts the character;
  - Backspace removes the last grapheme (`unicode-segmentation`);
  - `Input::Paste` inserts the text with control characters dropped;
  - the text is capped at 200 characters.

  Ctrl-Q still quits, because lookup runs for chords with Ctrl.
- Actions:
  - `NewRequest` and `NewFolder` open a prompt titled "New request in {folder}" (or "New folder in {folder}"). The parent is the selected folder, else the selected request's folder, else the root.
  - `Rename` opens a prompt with the current name.
  - `Delete` asks "Delete request {name}?" or "Delete folder {name} and its {n} requests?".
  - `MoveUp` and `MoveDown` queue `UiCommand::Op(Op::Move{…})` straight away.
  - `Reload` queues `UiCommand::Reload`.
  - `Confirm` on a prompt with non-blank text queues the op; `Confirm` on `Ask` queues its op. `Close` cancels either.
- `UiCommand::Op(Op)` and `UiCommand::Reload` go to `engine.apply(op)` and `engine.reload()`.
- `views/prompt.rs` draws a centred box at least 40 cells wide: the title, the text with a block cursor (truncated from the left with `…` when too long, measured by grapheme width), and the hint `Enter ok · Esc cancel`. `views/ask.rs` draws the same box with `Enter/y yes · Esc/n no`.
- Sidebar: `▾`/`▸` for open and collapsed folders, and the selected row reversed, folder rows included.
- Request pane for a folder: its name, path, the count of requests below, and its default headers and vars (values shown as written; no secrets live there).

- [ ] **Step 1: failing tests** in `app.rs` (main_view module) and `tree.rs`:
  - `rows_hide_children_of_collapsed_folders`;
  - `down_visits_folder_rows`, which replaces `down_skips_folder_rows`: folder rows are selectable now that the tree has folder actions. This is a ruling in the ledger;
  - `left_collapses_then_goes_to_the_parent`;
  - `enter_toggles_a_folder`;
  - `a_opens_a_prompt_and_enter_queues_new_request_in_the_folder`;
  - `letters_type_in_the_prompt_even_when_bound`: typing `et` in the prompt neither opens the picker nor toggles the trace, and the text is `et`;
  - `ctrl_q_quits_from_the_prompt`;
  - `rename_prefills_the_name`;
  - `delete_asks_and_y_confirms` and `esc_cancels_the_ask`;
  - `shift_k_queues_move_up`;
  - `a_created_path_is_selected_and_its_folder_expanded`;
  - `the_selection_follows_its_id_across_a_reload`;
  - snapshots `snapshot_160x40_tree_collapsed`, `snapshot_80x24_prompt` and `snapshot_80x24_ask`.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-tui`. Expected: FAIL. Commit `test: tree navigation, prompt and dialog`.
- [ ] **Step 3: implement.** Wire `UiCommand::Op` and `Reload` in `event_loop`.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Review every snapshot diff. Commit `feat: tree navigation, prompt and dialog`.

### Task 10: TUI — the command palette

**Files:** create `crates/pawsman-tui/src/views/palette.rs`, `crates/pawsman-tui/src/palette.rs` (items and ranking); modify `app.rs`, `views.rs`.

**Interfaces — produces:**

- `palette::Item` is either `Request { id, label: String }` or `Action { action, label: String, key: Option<String> }`.
  - A request's label is `{name}  {folder path}`; an invalid entry's label is its file path.
  - Actions are the `Main` actions except `Up`, `Down`, `PageUp`, `PageDown`, `Confirm`, `Expand` and `Collapse`. Their labels are capitalised hints ("Send", "Pick env", "New request"…).
- `palette::items(snapshot, keymap) -> Vec<Item>`, actions first, and `palette::filter(items, query) -> Vec<(Item, Match)>` through `fuzzy::rank`.
- `Overlay::Palette { query: String, at: usize }`. Ctrl-K opens it with an empty query.
  - Text entry is the Task 9 rule.
  - Up and Down move the highlight.
  - Enter on a request selects it, expands its ancestors and focuses the sidebar.
  - Enter on an action closes the palette and runs the action as if pressed in Main.
  - Esc closes it.
- `views/palette.rs`:
  - a box 60 % of the width (at least 40 cells), from the top third of the screen;
  - `› {query}` with a cursor;
  - rows: a tag (the method for a request, `act` for an action), the label with matched characters in bold accent, and the key on the right for actions;
  - the highlight reversed, and as many rows as fit;
  - `no match` when the list is empty.

- [ ] **Step 1: failing tests:**
  - `ctrl_k_opens_and_typing_filters`;
  - `j_and_k_type_in_the_palette`;
  - `enter_on_a_request_selects_it`;
  - `enter_on_an_action_runs_it`: the query `pick` and Enter open the environment picker;
  - `esc_closes_the_palette`;
  - `snapshot_100x30_palette`;
  - `tiny_palettes_never_panic`.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: command palette`.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: command palette`.

### Task 11: CLI — `pawsman init`, `keys.toml` and the end-to-end checks

**Files:** modify `crates/pawsman-cli/src/{args,main}.rs`; create `crates/pawsman-cli/tests/it/{tree,init,keys}.rs`, register them in `tests/it/main.rs`.

**Interfaces — produces:**

- `pawsman init [DIR]` (clap subcommand with `args_conflicts_with_subcommands`; `DIR` defaults to `.`):
  - calls `store::init`;
  - prints `created a collection in {dir}` on stdout (an `#[expect(clippy::disallowed_methods, reason = …)]` as for the exit line) and exits 0;
  - on error, prints the usual `pawsman: {code}: …` line and exits 1;
  - never starts the TUI.
- `main` loads `Keymap::load(&paths.keys_file())`; an error becomes a notice, and the result is passed through `Options::keymap`.

- [ ] **Step 1: failing tests:**
  - `init.rs` (`std::process::Command` on `env!("CARGO_BIN_EXE_pawsman")`):
    - `init_creates_a_collection_that_opens`: `store::load` on the result works;
    - `init_twice_fails_with_exists`.
  - `tree.rs` (PTY, temp collection, `XDG_*` in a temp dir):
    - `an_external_edit_shows_within_a_second`: rewrite `a.req.toml` with a new name; `wait_for_screen` sees it, and the test asserts under 1.5 s from the write to the PTY seeing it;
    - `broken_toml_becomes_an_error_node`: the row shows `!` and the file name, and selecting it shows `model::invalid_file`;
    - `rename_and_reorder_change_only_their_lines`:
      - The collection has `_folder.toml` `order = ["a", "b"]` with a comment, and `a.req.toml` with a comment and headers.
      - Do: `r`, clear the name, type `Alpha`, Enter, then `J`.
      - Afterwards `alpha.req.toml` exists, and its text equals the old text with only the `name` line replaced.
      - `_folder.toml` equals the old text with only the `order` line replaced, now `["b", "alpha"]`.
    - `delete_asks_first`: `d` shows the question, `n` keeps the file, then `d` and `y` remove it.
  - `keys.rs`: `a_keys_toml_override_rebinds_send` puts `keys.toml` with `send = ["F6"]` under the temp `XDG_CONFIG_HOME`. F6 sends (the fake receives), and F5 does not. `an_invalid_keys_toml_shows_a_notice_and_keeps_defaults`.
- [ ] **Step 2: run** `cargo nextest run -p pawsman-cli`. Expected: FAIL. Commit `test: init, keys.toml and tree end to end`.
- [ ] **Step 3: implement.**
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: init and keys.toml`.

### Task 12: fixtures, visual check, live run, docs, PR

- [ ] **Fixtures:** `fixtures/demo/_folder.toml` with `order = ["me", "users", "misc", "template"]`, so reordering has something to show.
- [ ] **PNG scenarios** (visual-check skill), reading every file:
  - the tree with a collapsed folder;
  - the folder summary;
  - the prompt;
  - the dialog;
  - the palette with highlighted matches at 80×24 and 160×48;
  - the help overlay with the new rows;
  - a light theme;
  - 256 colours.
- [ ] **Live run in kitty**, through the test window's own socket only. Run `pawsman-fake`, then `pawsman` on a copy of `fixtures/demo` in a temp dir, and:
  - edit a file with `sed -i` from outside, and see the change;
  - break and fix a request;
  - create, rename, move and delete a request and a folder, then compare with `git diff --no-index` against the original copy;
  - Ctrl-K, then `me`, then Enter, then F5.

  Then repeat over SSH and in tmux in the matrix box (inotify works inside the container).
- [ ] **Docs:**
  - README: `pawsman init`, the tree keys, Ctrl-K, `keys.toml` with an example.
  - ARCHITECTURE: the store and watcher flow as a Mermaid sequence (watcher batch → refresh → rebuild → snapshot; op → plan → apply → refresh).
  - DECISIONS:
    - stable ids by path;
    - empty folders through `_folder.toml`;
    - `toml_edit` for single-key edits;
    - own debounce instead of `notify-debouncer-full`;
    - own fuzzy matcher instead of `nucleo-matcher` (licence, `unsafe`);
    - non-recursive watches per walked directory (no inotify flood from `.git` or `node_modules`);
    - own writes ignored through the byte cache instead of a hash list;
    - folder delete keeps other files;
    - the environment remembered in `.pawsman/state.toml`;
    - renames move the file and the `order` entry but leave sidecars named as they are;
    - folder rows are selectable;
    - `keys.toml` all-or-nothing.
  - Error pages for `core::write`, `read`, `exists`, `watch` and `keys_invalid`.
  - CLAUDE.md: the current piece and the plan link.
  - Spec 1.4: the resolved decisions and the dependency row.
  - Issue #52: tick the sidebar O(n²) item.
- [ ] **Mutation testing:** `cargo mutants --in-diff` for `pawsman-model`, `pawsman-core` and `pawsman-tui`. Fix the meaningful gaps with tests.
- [ ] **Final review:** a fresh-context review of the whole branch (opus), with this plan's Review focus. Fix Critical and Important findings with failing tests first; minors go to a follow-up issue.
- [ ] **PR:**
  - It closes #13 (store and watcher) and #15 (layout, tree, palette).
  - It lists `notify` and `toml_edit` with their vetting facts, and the two dependencies the spec named that were not taken, with the reasons.
  - Merge on green CI.
