# M1.3 Format, environments and interpolation: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A collection is a tree of folders with the full request format, environments and secrets; every request is sent through a pure `interpolate` that resolves `{{…}}` from seven scopes and returns a trace. The TUI picks the environment and shows the trace, and no secret value ever reaches a log line.

**Architecture:** `pawsman-model` stays pure. It parses every file the store hands it (manifest, folders, requests, sidecars, environments, `.env`) into a `Collection` tree, writes requests back canonically, and owns `template`, `scope`, `interpolate`, `trace` and `Redactor`. `pawsman-core` walks the directory and reads the bytes. It builds the scopes from the active environment, the global file and the env snapshot, and encodes the resolved body into bytes for the wire (query, form, multipart, file). `pawsman-net` takes a byte body and a `Redactor` and logs and reports only redacted host and path. `pawsman-tui` gains folder rows, an environment picker and a trace view.

**Tech stack:** secrecy 0.10.3, uuid 1.29.0 (no default features; `Builder::from_random_bytes`), time 0.3.55 (`formatting`), fastrand 2.5.0; dev: proptest 1.12.0. Everything from 1.1 and 1.2 stays.

**Spec:** [M1 design, piece 1.3](../specs/2026-10-26-m1-http-client-design.md#13-format-environments-and-interpolation), with the format in [program spec §2](../specs/2026-10-23-pawsman-design.md#2-on-disk-format-variables-and-secrets).

## Global constraints

- Everything in the [1.1](2026-10-26-m1-1-binary-terminal-splash.md#global-constraints) and [1.2](2026-10-26-m1-2-thin-path.md#global-constraints) constraints still holds.
- `pawsman-model` does no I/O: no `std::fs`, no clock, no RNG. Builtins get their time and seed from the caller.
- Unknown keys in every file are an error (`model::invalid_file`); an unknown variable is `model::unknown_var`, never an empty string (rules/rust-api-client.md §3).
- Secrets are `secrecy::SecretString` from `.env` or the process env to the wire. A secret value never appears in a log line, a notice, an error message or the trace; the TUI masks it as `••••••`.
- Values read through `{{env.NAME}}` are treated as secrets for redaction: masked, and redacted in logs.
- Sidecar threshold: a `json` or `text` body above 40 lines, or one that is not UTF-8, is written to a sidecar.
- Writing a request is canonical: `parse(write(r)) == r` and `write(parse(write(r))) == write(r)` byte for byte.
- Dependencies are vetted (rules §1). secrecy has had no release since 2024-10, but it counts as an obviously finished crate: a small wrapper over zeroize, 188 M downloads, and commits in its repository in 2026-09. This goes in DECISIONS.

## Review focus

1. **A secret that reaches the URL percent-encoded** (a query value `{{token}}` holding `a b&c`): the log line, the error message and the trace show neither form (Task 5 `redactor_hides_raw_and_percent_encoded_forms`, Task 8 `a_secret_in_the_query_is_not_logged`).
2. **A variable bomb** (`a = "{{b}}{{b}}"`, `b = "{{c}}{{c}}"`, … 30 deep): interpolation stops at 1 MiB per field with `model::bad_template`. No hang and no out-of-memory (Task 5 `expansion_is_capped`).
3. **A collection directory that holds `.git`, `node_modules`, a symlink loop or deep nesting**: loading finishes fast. Hidden directories are skipped, symlinked directories are not followed, and depth is capped at 16 (Task 6 `symlinked_dirs_and_hidden_dirs_are_skipped`, `depth_is_capped`).
4. **A `.env` written on Windows or by other tools** (CRLF, a BOM, `export KEY=…`, quoted values, comments, a bad line): it parses, and the bad line is reported by line number without its value (Task 4 `dotenv_accepts_common_dialects`, `a_bad_dotenv_line_names_the_line_not_the_value`).
5. **The same header from a folder default and from the request in different case** (`accept` against `Accept`): the request's value wins and is sent once (Task 5 `request_headers_override_defaults_case_insensitively`).

---

## File map

| File | Responsibility |
| --- | --- |
| `crates/pawsman-model/src/format.rs` | `CURRENT`, `migrate`, reading `format` from a manifest |
| `crates/pawsman-model/src/manifest.rs` | `pawsman.toml`: name, `[vars]`, `[headers]` |
| `crates/pawsman-model/src/request.rs` | the full request file (parse) |
| `crates/pawsman-model/src/request/body.rs` | `Body`, `Content`, `Part`, sidecar references |
| `crates/pawsman-model/src/write.rs` | canonical request writer and sidecar placement |
| `crates/pawsman-model/src/folder.rs` | `_folder.toml`: name, `order`, defaults |
| `crates/pawsman-model/src/collection.rs` | the tree: folders, entries, ids, environments, `.env` |
| `crates/pawsman-model/src/source.rs` | `SourceFile`, the store's input to the model |
| `crates/pawsman-model/src/environment.rs` | `envs/*.env.toml`: `[vars]`, `secrets` |
| `crates/pawsman-model/src/dotenv.rs` | `.env` parsing into `SecretString`s |
| `crates/pawsman-model/src/secrets.rs` | which value a declared secret takes |
| `crates/pawsman-model/src/template.rs` | `{{…}}` parsing into segments |
| `crates/pawsman-model/src/scope.rs` | `Scopes`, `Source`, `Builtins` |
| `crates/pawsman-model/src/interpolate.rs` | `interpolate`, `ResolvedRequest`, `ResolvedBody` |
| `crates/pawsman-model/src/trace.rs` | `Trace`, `TraceEntry`, `Field`, `TraceValue` |
| `crates/pawsman-model/src/redact.rs` | `Redactor` |
| `crates/pawsman-core/src/store.rs` | walking the tree, reading bytes |
| `crates/pawsman-core/src/globals.rs` | `~/.config/pawsman/globals.toml` |
| `crates/pawsman-core/src/wire.rs` | encoding a resolved request into URL, headers and body bytes |
| `crates/pawsman-core/src/state.rs`, `engine.rs` | active environment, scopes, preview, `Unresolved` |
| `crates/pawsman-net/src/outgoing.rs`, `runtime.rs`, `send.rs` | byte bodies, redacted logs and errors |
| `crates/pawsman-tui/src/views/{sidebar,request,picker}.rs`, `app.rs` | folder rows, environment picker, trace |
| `fixtures/demo/` | folders, `envs/dev`, `envs/prod`, `.env`, sidecar |

---

### Task 1: model — templates and the new codes

**Files:** create `crates/pawsman-model/src/template.rs`, `docs/errors/model/bad_template.md`; modify `lib.rs`, `codes.rs`, `error.rs`, `Cargo.toml` (dev: `proptest`), root `Cargo.toml` (`proptest = "=1.12.0"`).

**Interfaces — produces:**

- `template::parse(text: &str) -> Result<Template<'_>, TemplateError>`; `Template` is a `Vec<Segment<'_>>`, read through `segments()`.
- `Segment::{Literal(Cow<'a, str>), Var { name: VarName<'a>, span: Range<usize> }}`. The span covers `{{` through `}}` in `text`.
- `VarName::{Plain(&str), Env(&str), Builtin(&str)}`, from `name`, `env.NAME` and `$name`. Whitespace inside the braces is trimmed.
- `TemplateError { span: Range<usize>, reason: &'static str }`, turned into `ModelError::bad_template(field: &str, err)`. The message is `` `{field}` has a bad template at {start}: {reason} ``.
- Rules:
  - `\{{` is a literal `{{`.
  - A lone `}}` is literal.
  - `{{` without a closing `}}` is "unterminated `{{`".
  - Empty braces `{{}}` are "empty variable".
  - A name allows `[A-Za-z0-9_.-]`; a `$` is allowed only as the first character. Anything else is "bad character in a variable name".
  - `has_vars(text) -> bool`.
- Codes: `BAD_TEMPLATE = "model::bad_template"` in `codes::ALL`, with its page.

- [ ] **Step 1: failing tests** in `template.rs` (stub `parse` returns `Ok(Template(vec![]))`):

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    fn names(text: &str) -> Vec<String> {
        parse(text).unwrap().segments().iter().filter_map(|s| match s {
            Segment::Var { name, .. } => Some(format!("{name:?}")),
            Segment::Literal(_) => None,
        }).collect()
    }

    #[test]
    fn vars_env_and_builtins_are_told_apart() {
        assert_eq!(
            names("{{base}}/u/{{ id }}?t={{env.HOME}}&r={{$uuid}}"),
            ["Plain(\"base\")", "Plain(\"id\")", "Env(\"HOME\")", "Builtin(\"uuid\")"],
        );
    }

    #[test]
    fn spans_cover_the_braces() {
        let t = parse("ab{{x}}cd").unwrap();

        assert_matches!(&t.segments()[1], Segment::Var { span, .. } if *span == (2..7));
    }

    #[test]
    fn an_escaped_open_is_literal() {
        let t = parse(r"a\{{b}}c").unwrap();

        assert_eq!(t.segments(), &[Segment::Literal("a{{b}}c".into())], "one literal");
    }

    #[rstest]
    #[case::unterminated("a{{b", 1, "unterminated `{{`")]
    #[case::empty("{{ }}", 0, "empty variable")]
    #[case::bad_char("{{a b}}", 0, "bad character in a variable name")]
    #[case::dollar_inside("{{a$b}}", 0, "bad character in a variable name")]
    fn bad_templates_say_where(#[case] text: &str, #[case] start: usize, #[case] reason: &str) {
        let err = parse(text).unwrap_err();

        assert_eq!((err.span.start, err.reason), (start, reason), "{text:?}");
    }

    #[test]
    fn a_lone_close_is_literal() {
        assert!(names("a}}b").is_empty(), "no variable");
    }

    proptest! {
        #[test]
        fn text_without_braces_is_one_literal(text in "[^{}\\\\]*") {
            let t = parse(&text).unwrap();
            let joined: String = t.segments().iter().map(|s| match s {
                Segment::Literal(l) => l.to_string(),
                Segment::Var { .. } => unreachable!(),
            }).collect();
            prop_assert_eq!(joined, text);
        }

        #[test]
        fn parse_never_panics(text in ".*") {
            let _ = parse(&text);
        }
    }
}
```

- [ ] **Step 2: run** `cargo nextest run -p pawsman-model template`. Expected: FAIL on the assertions, not on compilation. Commit `test: template parsing`.
- [ ] **Step 3: implement** a scanner over `text` that loops on `str::find("{{")` and stays on char boundaries. Adjacent literals merge so an escape produces one literal. Add `ModelError::bad_template` (`Kind::BadTemplate { field: String, at: usize, reason: &'static str }`, with the help text "Close every `{{` with `}}`, or write `\\{{` for a literal `{{`."), the code, and `docs/errors/model/bad_template.md` in the page template.
- [ ] **Step 4: run** the same tests and the registry test. Expected: PASS. Commit `feat: template parsing`.

### Task 2: model — format version and the manifest

**Files:** create `src/format.rs`, `src/manifest.rs`, `docs/errors/model/format_too_new.md`; modify `collection.rs` (uses `Manifest::parse`), `error.rs`, `codes.rs`, `docs/errors/model/invalid_file.md` (drop the "format other than 1" sentence).

**Interfaces — produces:**

- `format::CURRENT: u32 = 1`.
- `format::migrate(table: toml::Table, from: u32) -> Result<toml::Table, ModelError>`: `from == CURRENT` returns the table unchanged. `from > CURRENT` is `model::format_too_new` ("`{path}` uses format {from}; this pawsman reads up to {CURRENT}"). `from == 0` is `invalid_file`. It takes `path: &Path` as a third argument for the message.
- `Manifest::parse(text: &str, path: &Path) -> Result<Manifest, ModelError>`:
  1. Parses into a `toml::Table`.
  2. Reads `format`; missing or not an integer is `invalid_file` "`format` must be an integer".
  3. Removes `format` from the table and runs `migrate`.
  4. Deserializes `{ name: String, vars: Option<OrderedStrings>, headers: Option<OrderedStrings> }` with `deny_unknown_fields`.
- Getters: `name()`, `vars() -> &[(String, String)]`, `headers() -> &[(String, String)]`, `format() -> u32`.
- `OrderedStrings` (`pub(crate)`, in `manifest.rs`) is a `Vec<(String, String)>` deserialized from a TOML table in file order (`toml::Table` with `preserve_order`). A non-string value is an error naming the key, for example "`vars.port` must be a string". Tasks 3 and 4 reuse it for `[headers]`, `[vars]` and `[body.form]`.
- Code `FORMAT_TOO_NEW = "model::format_too_new"`.

- [ ] **Step 1: failing tests** in `manifest.rs` and `format.rs`:

```rust
#[test]
fn a_newer_format_is_too_new() {
    let err = Manifest::parse("format = 2\nname = \"x\"\n", Path::new("/c/pawsman.toml")).unwrap_err();

    assert_eq!(err.code(), codes::FORMAT_TOO_NEW, "code");
    assert!(err.to_string().contains("format 2"), "{err}");
}

#[test]
fn vars_and_headers_keep_file_order() {
    let m = Manifest::parse("format = 1\nname = \"D\"\n[vars]\nz = \"1\"\na = \"2\"\n[headers]\nX-B = \"b\"\nX-A = \"a\"\n", Path::new("m")).unwrap();

    assert_eq!(m.vars(), &[("z".into(), "1".into()), ("a".into(), "2".into())], "vars order");
    assert_eq!(m.headers()[0].0, "X-B", "headers order");
}

#[rstest]
#[case::no_format("name = \"x\"\n")]
#[case::zero("format = 0\nname = \"x\"\n")]
#[case::string_format("format = \"1\"\nname = \"x\"\n")]
#[case::unknown_key("format = 1\nname = \"x\"\nextra = 1\n")]
#[case::number_var("format = 1\nname = \"x\"\n[vars]\nport = 8080\n")]
fn bad_manifests_are_invalid_files(#[case] text: &str) {
    let err = Manifest::parse(text, Path::new("m")).unwrap_err();

    assert_eq!(err.code(), codes::INVALID_FILE, "{text:?}");
}

#[test]
fn migrate_is_the_identity_at_the_current_format() {
    let table: toml::Table = toml::from_str("name = \"x\"\n").unwrap();

    assert_eq!(migrate(table.clone(), CURRENT, Path::new("m")).unwrap(), table);
}
```

- [ ] **Step 2: run.** Expected: FAIL. Commit `test: format version and manifest`.
- [ ] **Step 3: implement.** Replace the manifest struct in `collection.rs` with `Manifest::parse`, and expose `Collection::manifest() -> &Manifest`. Add the error kind, the code and the page. Then reword `invalid_file.md`.
- [ ] **Step 4: run** `cargo nextest run -p pawsman-model`. Expected: PASS, and the old `a_broken_manifest_is_an_invalid_file` case `format = 2` now expects `FORMAT_TOO_NEW` (update that case: the spec moved the code). Commit `feat: format version and manifest`.

### Task 3: model — the full request file and its writer

**Files:** modify `src/request.rs`; create `src/request/body.rs`, `src/write.rs`, `src/write/snapshots/` (insta); modify `Cargo.toml` (dev: `insta`).

File format (program spec §2; `capture`, `assert`, `script` parse and are kept for M2; `auth` is kept for M3):

```toml
name = "Create user"
kind = "http"                      # optional, only "http"
method = "POST"                    # default GET
url = "{{base}}/users"
docs = """Creates a user."""       # optional markdown

[headers]
Accept = "application/json"

[query]
page = "1"
ids = ["1", "2"]                   # repeated key

[vars]
id = "42"

[body]                             # exactly one of:
json = """{"name": "ada"}"""       # or json = { sidecar = "create-user.body.json" }
# text = "…"                       # or text = { sidecar = "x.body.txt" }
# form = { a = "1", b = "2" }
# file = "upload.bin"              # path relative to this file
# [[body.multipart]]
# name = "avatar"                  # then exactly one of value / file
# file = "ada.png"
# content_type = "image/png"       # optional

[auth]                             # any table, kept verbatim
type = "bearer"

[[capture]]                        # any tables, kept verbatim
var = "etag"

[[assert]]
expr = "status == 200"

[script]
pre = "…"
post = "…"
```

**Interfaces — produces:**

- `Request` keeps `name()`, `method()`, `url()`, `headers()`, and adds:
  - `query() -> &[(String, Vec<String>)]`, `vars() -> &[(String, String)]`, `body() -> Option<&Body>`;
  - `docs() -> Option<&str>`, `auth() -> Option<&toml::Table>`, `captures() -> &[toml::Table]`, `asserts() -> &[toml::Table]`, `script() -> Option<&Script>`;
  - `has_m2_parts() -> bool`.
  - `has_template()` is removed.
- `Body::{Json(Content), Text(Content), Form(Vec<(String, String)>), Multipart(Vec<Part>), File(PathBuf)}`, with `Content::{Utf8(String), Binary(Vec<u8>)}`.
- `Part` has `name()`, `value() -> PartValue` (`PartValue::{Text(&str), File(&Path)}`) and `content_type() -> Option<&str>`. `Script` has `pre() -> Option<&str>` and `post() -> Option<&str>`.
- `Request::parse(text: &str, path: &Path, sidecar: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Request, ModelError>`. `sidecar(name)` returns the bytes of the file `name` next to `path`. `None` is `model::missing_sidecar` ("`{path}` names the body file `{name}`, which does not exist"). Sidecar bytes that are UTF-8 become `Content::Utf8`, others `Content::Binary`. A sidecar name containing `/` or `..` is `invalid_file`.
- `write::request(r: &Request, stem: &str) -> Written`, where `Written { toml: String, sidecar: Option<(String, Vec<u8>)> }`. `stem` is the file name without `.req.toml`.
  - Key order: `name`, `method`, `url`, `docs`, then `[headers]`, `[query]`, `[vars]`, `[body]`, `[auth]`, `[[capture]]`, `[[assert]]`, `[script]`.
  - `kind` is not written (http is the default).
  - `method` is always written.
  - Empty tables are omitted.
  - Multi-line strings use `"""`. Serialize with `toml::to_string` of an ordered `toml::Table`; the property test checks the output is stable.
  - A `Json` or `Text` content with more than 40 lines, or `Binary`, goes to `{stem}.body.json`, `{stem}.body.txt` or `{stem}.body.bin`.
  - A one-element query array is written as a string.
- Code `MISSING_SIDECAR = "model::missing_sidecar"`.

- [ ] **Step 1: failing tests**: in `request.rs`, parse cases for every section, each with its own test:
  - a full file (above) gives every getter;
  - `[body]` with two keys is `invalid_file` "`[body]` holds exactly one of json, text, form, multipart, file";
  - a multipart part with both `value` and `file`, or neither, is invalid;
  - a sidecar reference resolves through the closure;
  - a missing sidecar is `MISSING_SIDECAR`;
  - binary sidecar bytes give `Content::Binary`;
  - `sidecar = "../x"` is invalid;
  - `[script]` with an unknown key is invalid;
  - the old 1.2 cases stay.

  In `write.rs`:

```rust
#[test]
fn a_full_request_writes_canonically() {
    let r = Request::parse(FULL, Path::new("/c/create-user.req.toml"), &no_sidecars).unwrap();

    insta::assert_snapshot!(write::request(&r, "create-user").toml);
}

#[test]
fn a_long_body_moves_to_a_sidecar() {
    let body = (0..41).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
    let r = request_with(Body::Text(Content::Utf8(body.clone())));

    let w = write::request(&r, "notes");

    assert_eq!(w.sidecar.as_ref().map(|(n, b)| (n.as_str(), b.clone())), Some(("notes.body.txt", body.into_bytes())), "sidecar");
    assert!(w.toml.contains("text = { sidecar = \"notes.body.txt\" }"), "{}", w.toml);
}

proptest! {
    #[test]
    fn parse_write_parse_is_stable(r in arb_request()) {
        let w = write::request(&r, "r");
        let lookup = |name: &str| w.sidecar.as_ref().filter(|(n, _)| n == name).map(|(_, b)| b.clone());
        let again = Request::parse(&w.toml, Path::new("/c/r.req.toml"), &lookup).unwrap();

        prop_assert_eq!(&again, &r);
        prop_assert_eq!(write::request(&again, "r").toml, w.toml);
    }
}
```

`arb_request()` builds the strategy from these pieces:

- names, URLs, header and var keys and values from `"[ -~\u{80}-\u{10ffff}\n\"\\\\]{0,40}"` (keys: `[A-Za-z][A-Za-z0-9_-]{0,12}`, unique per table);
- query with 1–3 values per key;
- one body variant, with `Binary` from `any::<Vec<u8>>()` that fails UTF-8;
- an optional `docs`;
- an optional `auth` table of string values;
- 0–2 captures;
- an optional script.

`request_with(body)` builds the request from a `name = "n"\nurl = "http://h"` file through `parse`. The test-only setter for the body is `#[cfg(test)] fn with_body(self, Body) -> Self`.

- [ ] **Step 2: run.** Expected: FAIL (the snapshot is pending, the parse cases fail). Commit `test: full request file and writer`.
- [ ] **Step 3: implement.**
  - Serde structs with `deny_unknown_fields`.
  - `BodyFile` is a struct of five `Option`s, checked for exactly one.
  - `Content` in TOML is an untagged `String | { sidecar: String }`.
  - Query values are untagged `String | Vec<String>`.
  - The writer builds a `toml::Table` in key order and calls `toml::to_string`.
- [ ] **Step 4: run.** Read the `.snap.new` and accept it if it is canonical. Run the proptest with `PROPTEST_CASES=2000` once. Expected: PASS. Commit `feat: full request file and writer`.

### Task 4: model — folders, environments and `.env`

**Files:** create `src/source.rs`, `src/folder.rs`, `src/environment.rs`, `src/dotenv.rs`, `src/secrets.rs`, `docs/errors/model/missing_secret.md`; rewrite `src/collection.rs`; modify `Cargo.toml` (`secrecy = "=0.10.3"`, root too).

**Interfaces — produces:**

- `SourceFile::new(path: PathBuf, bytes: Vec<u8>)`. `path` is relative to the collection root, with `/` separators.
- `Collection::from_files(root: &Path, files: Vec<SourceFile>) -> Result<Collection, ModelError>` replaces the 1.2 signature. Each input file is one of:
  - the manifest `pawsman.toml`; none is `not_a_collection`;
  - `**/_folder.toml`;
  - `**/*.req.toml`;
  - sidecars `**/*.body.*`;
  - `envs/*.env.toml`;
  - `.env`.

  Anything else is ignored. A file that is not UTF-8 (other than a sidecar) is `invalid_file` "not UTF-8".
- Tree:
  - `Collection::root_folder() -> &Folder`.
  - `Folder` has `name()`, `path() -> &Path` (relative, empty for the root), `headers()`, `vars()`, `auth() -> Option<&toml::Table>`, `children() -> &[Node]`, and `error() -> Option<&ModelError>` for a broken `_folder.toml`.
  - `Node::{Folder(Folder), Entry(Entry)}`.
- Order:
  - Children are listed first in the folder's `order` (request stems and folder names), then the rest by file name, folders and requests mixed.
  - Unknown names in `order` are ignored.
  - Ids come from 1 in a pre-order walk.
  - `Collection::entries()` stays: every entry in that walk order.
  - `Collection::chain(id) -> Vec<&Folder>` returns the folders from the root down to the request's folder.
- `_folder.toml`: `{ name: Option<String>, order: Option<Vec<String>>, headers: Option<OrderedStrings>, vars: Option<OrderedStrings>, auth: Option<toml::Table> }` with `deny_unknown_fields`, run through `format::migrate`.
  - A broken `_folder.toml` makes every request under it `EntryKind::Invalid` with that error. Its sending would otherwise silently lose the defaults.
  - The folder still shows, and `error()` is set.
- Environments:
  - `Collection::environments() -> &[Environment]`, sorted by name.
  - `Environment` has `name()` (the file stem before `.env.toml`), `vars()`, `secrets() -> &[String]`, and `error() -> Option<&ModelError>`. A broken file gives an environment with only a name and an error.
  - The file holds `{ vars: Option<OrderedStrings>, secrets: Option<Vec<String>> }` with `deny_unknown_fields`.
  - A name must match `[A-Za-z0-9_-]+`; otherwise the file is ignored with an issue.
- `.env`:
  - `dotenv::parse(text: &str, path: &Path) -> (Vec<(String, SecretString)>, Vec<ModelError>)`.
  - Accepted forms: blank lines and `#` comments; `KEY=value` and `export KEY=value`; whitespace around `=` is trimmed.
  - Quoting: single quotes are literal. Double quotes turn `\n`, `\t`, `\"` and `\\` into the characters they name. An unquoted value ends at a space followed by `#`.
  - CRLF and a leading BOM are allowed.
  - A bad line is `invalid_file` ``".env:{line}: expected `KEY=value`"``, and the value is never echoed.
  - `Collection::dotenv() -> &[(String, SecretString)]`.
  - `Collection::issues() -> &[ModelError]` holds `.env` and environment-name issues.
- Secrets:
  - `secrets::resolve(name: &str, process: &dyn Fn(&str) -> Option<String>, dotenv: &[(String, SecretString)]) -> Option<SecretString>`.
  - The lookup order is process `PAWSMAN_SECRET_<NAME>`, then `.env` `name`.
  - `<NAME>` is `name` uppercased, with every character outside `[A-Z0-9]` turned into `_`.
- Code `MISSING_SECRET = "model::missing_secret"`. It is raised in Task 5; the page is added here with the code.
- A broken folder's requests carry a clone of the folder's own `invalid_file` error; no new error kind.

- [ ] **Step 1: failing tests** in each new file:
  - `collection.rs`:
    - `folders_nest_and_order_puts_named_children_first`: `order = ["b", "sub"]` among `a`, `b`, `sub/`, `c` gives `b`, `sub/…`, `a`, `c`, with ids in that walk order;
    - `chain_lists_folders_root_first`;
    - `a_broken_folder_file_invalidates_its_requests`;
    - `a_missing_sidecar_is_an_error_entry` (`MISSING_SIDECAR`);
    - `a_sidecar_in_a_subfolder_is_found_next_to_its_request`;
    - `files_that_are_not_utf8_are_invalid`;
    - `environments_sorted_and_broken_ones_kept_with_their_error`;
    - `unrelated_files_are_ignored`.
  - `dotenv.rs`: `dotenv_accepts_common_dialects`, a table test over:

```rust
const DIALECTS: &str = "\u{feff}# comment\r\nA=1\r\nexport B = two\nC='lit $x \\n'\nD=\"a\\nb\"\nE=plain # trailing\n\n";
// expects A=1, B=two, C=lit $x \n (backslash kept), D=a<newline>b, E=plain
```

and `a_bad_dotenv_line_names_the_line_not_the_value`: the input `"A=1\nthis-is-a-secret-without-equals\n"` gives one issue containing `.env:2` and not containing `this-is-a-secret`.

- `secrets.rs`: `process_env_wins_over_dotenv`, `name_is_mapped_to_an_env_var` (`client-secret` → `PAWSMAN_SECRET_CLIENT_SECRET`), `unset_everywhere_is_none`.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: folders, environments and .env`.
- [ ] **Step 3: implement.**
  - Build a `BTreeMap<PathBuf, Vec<u8>>` of every input, then walk folder paths.
  - The closure handed to `Request::parse` looks up `parent.join(name)`.
  - Keep `Collection::request(id)`.
  - Expose `Collection::entry(id) -> Option<&Entry>` and `Entry::folder() -> &Path`.
- [ ] **Step 4: run** `cargo nextest run -p pawsman-model`. Expected: PASS. The 1.2 `collection.rs` tests are rewritten to the new `from_files` signature, with the same assertions. Commit `feat: folders, environments and .env`.

### Task 5: model — scopes, interpolation, trace and redaction

**Files:** create `src/scope.rs`, `src/interpolate.rs`, `src/trace.rs`, `src/redact.rs`, `docs/errors/model/{unknown_var,var_cycle}.md`; modify `Cargo.toml` (`uuid = { version = "=1.29.0", default-features = false, features = ["std"] }`, `time = { version = "=0.3.55", default-features = false, features = ["std", "formatting"] }`, `fastrand = "=2.5.0"`), root `Cargo.toml`.

**Interfaces — produces:**

- `Source::{Runtime, Request, Folder(PathBuf), Environment(String), Collection, Global, Process, Builtin}`. Its `Display` gives `runtime`, `request`, `folder users/admin`, `env dev`, `collection`, `global`, `process env` and `built-in`.
- `Scopes` is built by `Scopes::new(builtins: Builtins)` with these methods:
  - `.layer(source: Source, vars: &[(String, String)])`, pushed from highest to lowest; core pushes request, folders nearest first, environment, collection, global;
  - `.secret(source: Source, name: &str, value: Option<SecretString>)` for a declared secret at that scope; `None` is declared but unset;
  - `.process(lookup: Box<dyn Fn(&str) -> Option<String> + Send + Sync>)`.

  The runtime layer is the first `layer` call when present; it is empty in M1.
- `Builtins` comes in two kinds:
  - `Builtins::live(now: SystemTime, seed: u64)`, where the values are:
    - `$uuid`: a v4 UUID from `uuid::Builder::from_random_bytes(rng)`, new per occurrence;
    - `$timestamp`: Unix seconds;
    - `$isoDate`: RFC 3339 UTC to the second, for example `2026-10-27T09:30:00Z`;
    - `$randomInt`: `0..=1000`;
  - `Builtins::preview()`, which renders `‹uuid›`, `‹timestamp›`, `‹isoDate›` and `‹randomInt›`.

  An unknown `$name` is `unknown_var`.
- `interpolate(request: &Request, chain: &[&Folder], collection: &Manifest, scopes: &Scopes) -> Result<(ResolvedRequest, Trace), ModelError>`:
  - Fields: the URL, header names and values (defaults merged collection → folders root-first → request), query keys and values, and body text (`Json` and `Text` with `Utf8`, form keys and values, multipart text values). `Binary`, `File` and multipart file paths are not templated.
  - A header from a lower level is replaced by a later one with the same name, compared case-insensitively. The replacing header keeps its own spelling and the position of the first.
  - A variable's value is itself a template, resolved against the full stack. This allows recursion with a visiting stack: a repeat is `model::var_cycle` "`a` → `b` → `a`".
  - Nesting deeper than 64 is `bad_template` "nested more than 64 deep".
  - A field that grows past 1 MiB is `bad_template` "expands to more than 1 MiB".
  - An unknown name is `model::unknown_var` "`{{name}}` in {field} is not defined in any scope". `{{env.NAME}}` with `NAME` unset is `unknown_var` naming `env.NAME`.
  - A declared but unset secret is `model::missing_secret` "secret `{name}` is declared in env {env} but not set; set PAWSMAN_SECRET_{NAME} or add it to .env".
- `ResolvedRequest` has `method()`, `url()`, `headers()`, `query() -> &[(String, String)]` (flattened), `body() -> Option<&ResolvedBody>`, `redactor() -> &Redactor` and `base_dir() -> &Path` (the request file's folder relative to the root).
- `ResolvedBody::{Json(String), Text(String), Binary(Vec<u8>), Form(Vec<(String, String)>), Multipart(Vec<ResolvedPart>), File(PathBuf)}`. `ResolvedPart` has `name()`, `value() -> ResolvedPartValue` (`Text(&str) | File(&Path)`) and `content_type()`.
- `Trace` has `entries() -> &[TraceEntry]`. A `TraceEntry` has:
  - `field() -> &Field`, where `Field` is `Url`, `HeaderName(String)`, `HeaderValue(String)`, `QueryKey(String)`, `QueryValue(String)`, `Body` or `Var(String)`, the last for nested uses;
  - `span() -> Range<usize>`, in that field's template text;
  - `name() -> &str`, as written, such as `base`, `env.HOME` or `$uuid`;
  - `source() -> &Source`;
  - `value() -> &TraceValue`, `TraceValue::{Plain(String), Secret}`.

  Values from secrets and from `env.*` are `Secret`.
- `Redactor` is built by `Redactor::new(values: Vec<SecretString>)`, ignoring empty ones. Its methods:
  - `redact(&self, text: &str) -> Cow<'_, str>` replaces each secret, raw and in both percent-encoded forms (query `%20` and form `+`, from `redact::encode_query` and `redact::encode_form`, the same encoders `wire.rs` uses), with `***`;
  - `mask(&self, text) -> Cow<str>` does the same with `••••••`;
  - `is_empty()`.

  Longest secrets are matched first. `ResolvedRequest::redactor()` holds every secret and `env.*` value the request used.
- `pub fn encode_query(s: &str) -> String` and `pub fn encode_form(s: &str) -> String` live in `redact.rs`. The unreserved set is `A-Za-z0-9-._~`; everything else becomes `%XX` per UTF-8 byte, and `encode_form` turns a space into `+`.
- Codes `UNKNOWN_VAR` and `VAR_CYCLE`, with pages.

- [ ] **Step 1: failing tests** (`interpolate.rs` unless noted). Helpers: `req(toml)` parses a request, `scopes()` builds `Scopes::new(Builtins::live(UNIX_EPOCH + 1_800_000_000 s, 7))`.
  - `highest_scope_wins`: `base` set in collection, environment `dev` and request; the request value is used and the trace says `request`. Without the request var, `env dev`.
  - `folder_chain_nearest_first`.
  - `env_dot_reads_the_process_lookup_only`: a plain `{{HOME}}` is unknown even when the process has `HOME`.
  - `unknown_var_names_the_variable_and_field`.
  - `nested_values_resolve_and_trace_both`: the URL `{{api}}` with `api = "{{base}}/v1"` gives entries for `api` (`Url`) and `base` (`Var("api")`).
  - `a_cycle_is_var_cycle`: `a` → `b` → `a`, and the message contains `` `a` → `b` → `a` ``.
  - `expansion_is_capped`: 30 levels of `{{x}}{{x}}` give `BAD_TEMPLATE` within 1 s (assert with `Instant`).
  - `a_declared_unset_secret_is_missing_secret`.
  - `secrets_resolve_and_are_secret_in_the_trace`: the value is used in the resolved header, `TraceValue::Secret`, and `redactor().redact(url)` hides it.
  - `request_headers_override_defaults_case_insensitively`: folder `accept = "a"`, request `Accept = "b"` gives `[("Accept", "b")]` in the folder header's position.
  - `builtins_are_deterministic_for_a_seed_and_fresh_per_occurrence`: two `{{$uuid}}` differ, the same seed gives the same values, `$isoDate` is `2027-01-15T08:00:00Z`, and `$timestamp` is `1800000000`.
  - `preview_builtins_are_placeholders`.
  - `escaped_braces_survive_into_the_wire`: `\{{x}}` gives `{{x}}`.
  - `only_utf8_bodies_are_templated`: a `Binary` body passes byte for byte.
  - `redact.rs`: `redactor_hides_raw_and_percent_encoded_forms` (the secret `a b&c` in `?q=a%20b%26c`, `q=a+b%26c` and raw), `longest_first` (secrets `ab` and `abc`), and `empty_secrets_are_ignored`.
  - Property: `interpolate_never_panics` over random URL text from `"[{}a-z$. \\\\]{0,30}"` with a fixed scope set.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: interpolation, trace and redaction`.
- [ ] **Step 3: implement.**
  - Resolution works on `template::parse` segments, through a `Resolver { scopes, stack: Vec<String>, trace: Vec<TraceEntry>, secrets: Vec<SecretString> }` with `resolve_field(field, text) -> Result<String, ModelError>`.
  - Values are memoised per name within one call, so a diamond does not re-resolve.
  - Before every push, the size is checked against `1 << 20`.
  - `time::OffsetDateTime::from(now).replace_nanosecond(0)` is formatted with `Rfc3339`.
  - `fastrand::Rng::with_seed(seed)` supplies the 16 UUID bytes and the ints.
- [ ] **Step 4: run** `cargo nextest run -p pawsman-model`. Expected: PASS. Commit `feat: interpolation, trace and redaction`.

### Task 6: core — walking the collection and the global file

**Files:** rewrite `crates/pawsman-core/src/store.rs`; create `src/globals.rs`; modify `paths.rs` (`globals_file()`), `lib.rs`.

**Interfaces — produces:**

- `store::load(dir: &Path) -> Result<Collection, ModelError>`, which walks `dir`:
  - It visits files as `SourceFile`s with relative `/` paths.
  - It skips directories whose names start with `.` (so `.git` and `.pawsman`), plus `node_modules` and `target`. The file `.env` is kept.
  - It does not follow symlinked directories (`entry.file_type()` is not followed); symlinked files are read.
  - Depth is capped at 16: deeper directories are skipped with one `warn!(dir, "collection deeper than 16 levels; rest skipped")`.
  - At most 10 000 files are read; the rest are skipped with a warning.
  - Only names the model wants are read: `pawsman.toml`, `_folder.toml`, `*.req.toml`, `*.body.*`, `envs/*.env.toml` and `.env`.
  - Unreadable files are logged and skipped, as in 1.2.
  - It logs `info!(dir, requests, folders, environments, issues, ms, "collection loaded")` and one `warn!` per `Collection::issues()` entry, through its `Display`, which never carries a value.
- `globals::load(path: &Path) -> Result<Vec<(String, String)>, CoreError>`:
  - A missing file gives an empty list.
  - The file is `{ vars: Option<table of strings> }` with `deny_unknown_fields`.
  - Anything else is the new `core::globals_invalid` (with a page, "`{path}` is not valid: {reason}").
- `Paths::globals_file() -> PathBuf` is `config_dir/globals.toml`.

- [ ] **Step 1: failing tests** in `store.rs` (tempdir), each its own test:
  - `loads_the_tree_with_folders_envs_and_sidecars`;
  - `symlinked_dirs_and_hidden_dirs_are_skipped`: a `loop -> .` symlink (`#[cfg(unix)]`), plus `.git/x.req.toml` and `node_modules/y.req.toml`, which are not loaded;
  - `depth_is_capped`: 20 nested folders, each with a request; 16 levels load, and the test finishes under 2 s;
  - `dotenv_is_read`.

  In `globals.rs`: `missing_globals_are_empty`, `globals_vars_load`, `bad_globals_are_globals_invalid`.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: collection tree on disk and globals`.
- [ ] **Step 3: implement** with an explicit stack instead of recursion, then add the code, the page and `paths`.
- [ ] **Step 4: run** `cargo nextest run -p pawsman-core`. Expected: PASS. Commit `feat: collection tree on disk and globals`.

### Task 7: net — byte bodies and redacted logs and errors

**Files:** modify `crates/pawsman-net/src/outgoing.rs`, `runtime.rs`, `send.rs`, `error.rs`, `Cargo.toml` (`secrecy` is not needed; the `Redactor` comes from `pawsman-model`).

**Interfaces — produces:**

- `Outgoing::new(method: &str, url: &str, headers: &[(String, String)], body: Bytes, redactor: Redactor) -> Result<Outgoing, NetError>`. The 1.2 `Option<&str>` body becomes `Bytes`, and an empty body sends no body.
- `NetError::invalid_request(what, value)` is now built with the redacted value.
- `Outgoing::log_host() -> Cow<'_, str>` and `log_path() -> Cow<'_, str>` are the redacted host and path for every `info!`/`warn!` in `runtime.rs` and `send.rs`, and for every error that names the host (`dns`, `connect`, `tls`). `host()` stays `pub(crate)` for DNS, TLS and the `Host` header.

- [ ] **Step 1: failing tests**:
  - `outgoing.rs`: `invalid_values_are_redacted_in_errors` (a header value `"tok\nen"` where the secret is `tok\nen`: the message has `***`, not the secret).
  - `log_host_and_path_are_redacted`.
  - `runtime.rs` with a capturing `tracing` subscriber (`tracing_subscriber::fmt().with_writer(…)` over an `Arc<Mutex<Vec<u8>>>`, set with `tracing::subscriber::with_default`; add `tracing-subscriber` as a dev-dependency of `pawsman-net` and `pawsman-core`): `a_secret_in_the_path_is_not_logged`. It sends to `FakeHttp` at `/u/s3cr3t`, waits for `Done`, and asserts the captured log has `/u/***` and not `s3cr3t`.
  - `dns_errors_redact_the_host`, with the host `s3cr3t.invalid` as a secret.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: redacted net logs and errors`.
- [ ] **Step 3: implement.** Update every `Outgoing::new` call site in core (`state.rs` passes `Bytes::new()` and `Redactor::default()` until Task 8) and in the tests.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: redacted net logs and errors`.

### Task 8: core — environments, scopes, wire encoding and preview

**Files:** create `crates/pawsman-core/src/wire.rs`, `docs/errors/core/body_file.md`; modify `state.rs`, `engine.rs`, `error.rs`, `codes.rs`, `env.rs` (`EnvSnapshot::lookup(&self) -> impl Fn(&str) -> Option<String>`, a cloned `Arc` of the map), `Cargo.toml` (`fastrand`, `bytes`).

**Interfaces — produces:**

- `wire::encode(resolved: &ResolvedRequest, root: &Path, boundary_seed: u64) -> Result<Wire, CoreError>`, returning `Wire { url: String, headers: Vec<(String, String)>, body: Bytes }`:
  - Query pairs are appended to the URL with `?`, or `&` when the URL already has a `?`, through `encode_query`.
  - JSON gets `Content-Type: application/json` unless a `Content-Type` header is set (case-insensitive); text gets `text/plain; charset=utf-8`.
  - A form is `a=1&b=2` through `encode_form`, with `application/x-www-form-urlencoded`.
  - Multipart:
    - `multipart/form-data; boundary=pawsman-<24 alphanumerics from the seed>`. It always sets its own `Content-Type` unless the user's value already contains `boundary=`.
    - A text part is `Content-Disposition: form-data; name="…"`.
    - A file part adds `; filename="<file name>"` and `Content-Type` (its own or `application/octet-stream`). The file is read from `root.join(base_dir).join(path)`.
  - A file body is read the same way, with `application/octet-stream`.
  - A file that cannot be read is the new `core::body_file` "`{path}` cannot be read for the body: {io error}".
- `Setup` builder for the engine, replacing `Engine::start`'s arguments:
  - `Setup::new(settings: Settings)` with `.dir(Option<PathBuf>)`, `.env(EnvSnapshot)` and `.globals(Option<PathBuf>)`;
  - `Engine::start(setup: Setup, on_snapshot) -> Result<Engine, NetError>`;
  - `Engine::select_env(&self, env: Option<String>)`.
- `Event::SelectEnv(Option<String>)`:
  - An unknown or broken environment is ignored with a notice.
  - Selecting an environment clears nothing.
  - It logs `info!(env, "environment selected")`.
- `AppState::new(load, globals: Vec<(String, String)>, env: EnvSnapshot)`. A broken globals file becomes a notice in the engine and empty vars.
- Send:
  1. The scopes are request vars, the folders nearest first, the active environment's vars and secrets (each `secrets::resolve` with the env snapshot and the `.env`), the collection vars, the globals, and the process lookup. Builtins are `live(SystemTime::now(), fastrand::u64(..))`; the time and seed are taken at the effect boundary and passed into `apply` with `now`, so `apply` stays pure.
  2. `interpolate`.
  3. `wire::encode` with root and seed.
  4. `Outgoing::new` with the redactor.
  - A model or core error gives `RequestStatus::Unresolved(Notice)`, holding the code and message already formatted; the message never contains a value.
  - It logs `debug!(id, vars = trace.entries().len(), scopes = ?distinct sources, "interpolated")`.
- `Event::Send(id)` becomes `Event::Send { id, at: SystemTime, seed: u64 }`; the engine fills both.
- `Snapshot` adds:
  - `environments() -> &[Environment]` and `active_env() -> Option<&str>`;
  - `preview(id) -> Option<Result<Preview, Notice>>`, which runs the same scopes with `Builtins::preview()`.
  - `Preview` has `method()`, plus `url()`, `headers()` and `body_text() -> Option<String>`, all masked through `Redactor::mask`. It also has `trace() -> &Trace`, `has_m2_parts() -> bool` and `env() -> Option<&str>`.
- Notices: `Collection::issues()` become notices on load, and the 1.2 "variables are not resolved yet" notice is removed.

- [ ] **Step 1: failing tests**:
  - `wire.rs`, each its own test:
    - `query_is_appended_and_encoded`, with `?x=1` already present giving `&`;
    - `json_and_text_get_a_default_content_type_unless_set`;
    - `form_is_urlencoded`;
    - `multipart_text_and_file_parts`, with an exact byte snapshot from the seed (insta, `assert_snapshot!(String::from_utf8_lossy(..))`);
    - `a_missing_file_is_body_file`.
  - `state.rs`:
    - `sending_resolves_against_the_active_environment`: a collection with `envs/dev` (`base = http://dev`) and `envs/prod`; the `Effect::Send` URL is `http://dev/u`, and after `SelectEnv(prod)` it is `http://prod/u`;
    - `an_unknown_var_is_unresolved_and_sends_nothing`;
    - `a_missing_secret_is_unresolved_with_the_env_name`;
    - `a_secret_from_the_process_env_is_used_and_masked_in_preview`, with `PAWSMAN_SECRET_TOKEN=s3cr3t` in `EnvSnapshot::from_pairs`: the effect's header has `s3cr3t`, the preview header shows `••••••`, and the preview trace has `TraceValue::Secret`;
    - `selecting_a_broken_env_is_a_notice`;
    - `preview_uses_placeholders_for_builtins`.
  - `a_secret_in_the_query_is_not_logged`: an engine-level test with `FakeHttp` and a captured subscriber. It sends `?t={{token}}` with the token `a b&c`; neither `a b&c` nor `a%20b%26c` is in the log.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: environments, scopes and wire encoding`.
- [ ] **Step 3: implement.** Update the TUI's `Engine::start` call (`app.rs`) to `Setup`, with `Options::globals(Option<PathBuf>)` set by the CLI from `paths.globals_file()`.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: environments, scopes and wire encoding`.

### Task 9: TUI — folders, environment picker and trace

**Files:** modify `crates/pawsman-core/src/keys.rs` (actions and context), `crates/pawsman-tui/src/views/sidebar.rs`, `views/request.rs`, `views/main.rs`, `app.rs`; create `views/picker.rs`.

**Interfaces — produces:**

- Keys:
  - `Action::PickEnv` (`e`, in the bar as `env`), `Action::Trace` (`t`, in the bar as `trace`), `Action::Confirm` (Enter, in `Context::Picker`);
  - `Context::Picker`, where Up, Down, `k`, `j`, Enter and Esc (Close) apply.
  - The Main bar reads `F5 send · Tab pane · e env · t trace · Esc cancel · Ctrl-Q quit · F1 help`.
  - The Cyrillic fold makes `у` act as `e` and `е` act as `t`, through the existing fold table; a test checks it.
- Sidebar:
  - The first row is `env: dev` (or `env: none`) in the accent colour, then the tree.
  - Folder rows are `▾ name` (dim, indented two cells per level), and a broken folder shows `! name` in red.
  - Request rows are indented under their folder.
  - Up and Down move over request rows only, skipping folders.
- Picker (`views::picker::draw(frame, area, items, selected)`):
  - A list overlay anchored under the sidebar's env row: `none`, then the environments in order. A broken environment shows `! name (invalid)` and cannot be chosen.
  - Enter queues `UiCommand::SelectEnv(Option<String>)`, which the loop hands to `Engine::select_env`.
- Request pane:
  - It draws `Snapshot::preview(selected)`: the resolved, masked method and URL, then the headers and the body.
  - When the preview is an error, it shows the template as written and one red line with `code: message`.
  - When `has_m2_parts()`, it adds a dim line: "captures, asserts and scripts run from M2".
  - `t` toggles the trace section under the body: one line per entry, `name  ← source  = value`. `value` is `••••••` for a secret, and nested entries are indented under their parent field.
- Response pane: `RequestStatus::Unresolved(notice)` is drawn like `Failed`.

- [ ] **Step 1: failing tests**: insta `TestBackend` snapshots at 160×40 for:
  - a tree with two folders;
  - the picker open with a broken environment;
  - the trace on, with a secret;
  - an unresolved request.

  Plus these tests:

```rust
#[test]
fn a_secret_is_never_drawn() {
    let snap = snapshot_with_secret("s3cr3t");

    let text = render_with_trace(&snap, 160, 40);

    assert!(!text.contains("s3cr3t"), "masked:\n{text}");
    assert!(text.contains("••••••"), "mask shown");
}

#[test]
fn down_skips_folder_rows() {
    let mut app = app_with_tree(&["a/one", "b/two"]);
    app.selected = Some(id(1));

    app.update(Msg::Key(down()));

    assert_eq!(app.selected, Some(id(2)), "from a/one straight to b/two, past the `b` folder row");
}

#[test]
fn enter_in_the_picker_queues_select_env() {
    let mut app = app_with_envs(&["dev", "prod"]);

    for key in [char_key('e'), down(), enter()] {
        app.update(Msg::Key(key));
    }

    assert_eq!(app.take_commands(), [UiCommand::SelectEnv(Some("dev".into()))], "none → dev");
}
```

`app_with_tree`, `app_with_envs`, `char_key`, `down` and `enter` are new helpers in `app.rs`'s test module: an `App` on `Screen::Main` holding a snapshot built from those paths or environment names, and the matching `KeyEvent`s.

- [ ] **Step 2: run.** Expected: FAIL, with pending snapshots. Commit `test: folders, environment picker and trace`.
- [ ] **Step 3: implement.** Cache the preview in `App` by `(snapshot generation, selected id)` so it is not recomputed each frame.
- [ ] **Step 4: run.** Read every `.snap.new` and accept the right ones. Expected: PASS. Commit `feat: folders, environment picker and trace`.

### Task 10: fixtures, fake routes and the end-to-end check

**Files:**

- Modify `fixtures/demo/` as follows:
  - Move the requests into `users/` (with `_folder.toml`: `order`, `[headers] Accept`), `misc/`, and the root.
  - Make `url = "{{base}}/…"` everywhere.
  - Add `envs/dev.env.toml` (`base = "http://127.0.0.1:8787"`, `secrets = ["token"]`) and `envs/prod.env.toml` (`base = "http://localhost:8787"`, `secrets = ["token"]`).
  - Add `.env` (`token=demo-token`), plus `.gitignore` with `!fixtures/demo/.env`, and the demo README line saying the token is a placeholder.
  - Add `me.req.toml` (`Authorization = "Bearer {{token}}"`) and a `users/create.req.toml` with a 45-line JSON body in `create.body.json`.
  - Keep `template.req.toml` with `{{nope}}`, now an unknown variable.
- Modify `crates/pawsman-test-support/src/bin/pawsman-fake.rs`: `GET /me` answers 200 `{"user":"demo"}` for `Bearer demo-token`, else 401, and the bin learns how to check the header.
- Create `crates/pawsman-cli/tests/it/envs.rs`.

- [ ] **Step 1: failing PTY tests** (`envs.rs`), using a temp collection pointing at `FakeHttp`, with `XDG_STATE_HOME` set to a temp dir so the log lands there:
  - `switching_env_changes_the_resolved_url`: `e`, `Down`, Enter for `dev`; the request pane shows the dev URL. Then `e`, `Down`, `Down`, Enter for `prod` shows the prod URL.
  - `the_trace_names_each_scope`: `t` shows `← env dev` and `← collection`.
  - `the_secret_never_reaches_the_log`: with `PAWSMAN_SECRET_TOKEN=pawsman-fake-s3cr3t`, select `dev`, F5 on `me` until `200 OK`, then Ctrl-Q. Grep every file under the log dir: neither `pawsman-fake-s3cr3t` nor its percent-encoded form appears. The screen never shows it either.
  - `an_unknown_variable_shows_its_code`: shows `model::unknown_var`.
- [ ] **Step 2: run.** Expected: FAIL. Commit `test: environments end to end`.
- [ ] **Step 3: implement** the fixtures and the `/me` route, plus whatever wiring the tests expose.
- [ ] **Step 4: run** `cargo nextest run --workspace`. Expected: PASS. Commit `feat: demo environments and secret`.

### Task 11: visual check, live run, docs, PR

- [ ] **PNG scenarios** through the `visual-check` skill: the tree, the picker, the trace with a secret, an unresolved request, and the 60×20 single pane with the env row. Read every file.
- [ ] **Live run in kitty:** `pawsman-fake`, then `pawsman fixtures/demo`.
  - Pick dev and send `me` (expect 200).
  - Pick none and send `me` (expect `unknown_var` `base`).
  - Pick prod and send.
  - Open the trace on `users/create` (the sidecar body is shown).
  - Then repeat over SSH and in tmux in the matrix box.
  - Send keys only into the test window: `kitty @ send-text --match` and `kitty @ get-text`, never wtype into a focused window.
- [ ] **Docs:**
  - README: environments, `.env`, `PAWSMAN_SECRET_*`.
  - ARCHITECTURE §4: the scopes as a Mermaid flowchart.
  - DECISIONS, with these entries:
    - `[vars]` tables at the request, folder, collection, environment and global levels;
    - JSON bodies as strings until 1.6;
    - the sidecar syntax `{ sidecar = "…" }`;
    - a broken `_folder.toml` invalidates its requests;
    - the `env.*` values redacted;
    - the 1 MiB and 64-deep caps;
    - `core::body_file`;
    - secrecy as a finished crate;
    - no environment active at start (remembered with UI state in 1.4);
    - store skip rules.
  - Error pages for every new code.
  - CLAUDE.md: the current piece and the plan link.
  - Spec 1.3: the resolved decisions.
- [ ] **Mutation testing:** `cargo mutants --in-diff` for `pawsman-model` and `pawsman-core`. Fix what matters with tests.
- [ ] **Final review:** a fresh-context review of the whole branch (opus). Fix Critical and Important findings with failing tests first; minors go to a follow-up issue.
- [ ] **PR:**
  - It closes #11 (format and migrations) and #12 (environments, scopes, interpolation).
  - It lists the four dependencies with the vetting facts.
  - Merge on green CI.
