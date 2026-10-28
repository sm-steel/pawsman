//! Single-key changes to collection files that keep every other byte: comments, blank lines and
//! key order survive, so `git diff` shows only the line that changed.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Value};

use crate::ModelError;

/// Sets the top-level `name`; empty `text` gives a file holding only `name`.
///
/// # Errors
///
/// `model::invalid_file` when `text` is not TOML.
pub fn set_name(text: &str, path: &Path, name: &str) -> Result<String, ModelError> {
    let mut doc = document(text, path)?;
    set_value(&mut doc, "name", Value::from(name));
    Ok(doc.to_string())
}

/// Sets `order` to `order` as one inline array, in place or after the other top-level keys.
///
/// # Errors
///
/// `model::invalid_file` when `text` is not TOML.
pub fn set_order(text: &str, path: &Path, order: &[String]) -> Result<String, ModelError> {
    let mut doc = document(text, path)?;
    set_value(&mut doc, "order", Value::Array(order.iter().collect()));
    Ok(doc.to_string())
}

/// Replaces `from` with `to` in `order`; `None` when `from` is not listed.
///
/// # Errors
///
/// `model::invalid_file` when `text` is not TOML.
pub fn rename_in_order(
    text: &str,
    path: &Path,
    from: &str,
    to: &str,
) -> Result<Option<String>, ModelError> {
    edit_order(text, path, from, |keys, at| {
        if let Some(key) = keys.get_mut(at) {
            to.clone_into(key);
        }
    })
}

/// Drops `key` from `order`; `None` when it is not listed.
///
/// # Errors
///
/// `model::invalid_file` when `text` is not TOML.
pub fn remove_from_order(text: &str, path: &Path, key: &str) -> Result<Option<String>, ModelError> {
    edit_order(text, path, key, |keys, at| {
        keys.remove(at);
    })
}

/// A new GET request named `name`, with an empty URL.
#[must_use]
pub fn new_request(name: &str) -> String {
    format!("name = {}\nmethod = \"GET\"\nurl = \"\"\n", quoted(name))
}

/// A new `_folder.toml` naming the folder.
#[must_use]
pub fn new_folder(name: &str) -> String {
    format!("name = {}\n", quoted(name))
}

/// A new `pawsman.toml` for the current format.
#[must_use]
pub fn new_manifest(name: &str) -> String {
    format!(
        "format = {}\nname = {}\n",
        crate::format::CURRENT,
        quoted(name)
    )
}

/// A file or directory name for `name`: lowercase, letters, digits, `_` and `-`, at most 64
/// characters; empty when nothing is left.
#[must_use]
pub fn slug(name: &str) -> String {
    const MAX: usize = 64;
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let c = if c.is_alphanumeric() || c == '_' {
            c
        } else {
            '-'
        };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    let capped: String = out.chars().take(MAX).collect();
    capped.trim_end_matches('-').to_owned()
}

fn document(text: &str, path: &Path) -> Result<DocumentMut, ModelError> {
    text.parse::<DocumentMut>()
        .map_err(|e| ModelError::invalid_file(path, e.message()))
}

/// Replaces the value of a top-level `key`, keeping the comments around it; adds the key otherwise.
fn set_value(doc: &mut DocumentMut, key: &str, mut value: Value) {
    if let Some(Item::Value(old)) = doc.get_mut(key) {
        *value.decor_mut() = old.decor().clone();
        *old = value;
    } else {
        value.decor_mut().set_prefix(" ");
        doc.insert(key, Item::Value(value));
    }
}

/// Runs `change` on the `order` keys when `key` is listed, and writes the array back.
fn edit_order(
    text: &str,
    path: &Path,
    key: &str,
    change: impl FnOnce(&mut Vec<String>, usize),
) -> Result<Option<String>, ModelError> {
    let mut doc = document(text, path)?;
    let Some(array) = doc.get("order").and_then(Item::as_array) else {
        return Ok(None);
    };
    let mut keys: Vec<String> = array
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let Some(at) = keys.iter().position(|k| k == key) else {
        return Ok(None);
    };
    change(&mut keys, at);
    set_value(&mut doc, "order", Value::Array(keys.iter().collect()));
    Ok(Some(doc.to_string()))
}

/// `value` as a TOML basic string.
fn quoted(value: &str) -> String {
    Value::from(value).to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;
    use rstest::rstest;

    use super::*;
    use crate::{Collection, Manifest, Node, Request, SourceFile};

    const P: &str = "x.req.toml";

    fn path() -> &'static Path {
        Path::new(P)
    }

    /// The lines of `after` that differ from `before`, which must have as many lines.
    fn changed(before: &str, after: &str) -> Vec<(String, String)> {
        assert_eq!(
            before.lines().count(),
            after.lines().count(),
            "line count:\n{after}"
        );
        before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect()
    }

    const REQUEST: &str = "# who is calling\nname = \"Me\" # shown in the tree\nurl = \"{{base}}/me\"\n\n[headers]\n# auth\nAuthorization = \"Bearer {{token}}\"\n";

    #[test]
    fn set_name_keeps_comments_and_layout() {
        let after = set_name(REQUEST, path(), "Who am \"I\"").unwrap();

        assert_eq!(
            changed(REQUEST, &after),
            [(
                "name = \"Me\" # shown in the tree".to_owned(),
                "name = 'Who am \"I\"' # shown in the tree".to_owned()
            )],
            "{after}"
        );
    }

    #[test]
    fn set_name_on_an_empty_file_gives_just_the_name() {
        assert_eq!(set_name("", path(), "U").unwrap(), "name = \"U\"\n");
    }

    const FOLDER: &str = "name = \"Users\"\n# by hand\norder = [\"b\", \"a\"]\n\n[headers]\nAccept = \"application/json\"\n";

    #[test]
    fn set_order_replaces_one_line() {
        let order = ["a".to_owned(), "b".to_owned(), "c".to_owned()];

        let after = set_order(FOLDER, path(), &order).unwrap();

        assert_eq!(
            changed(FOLDER, &after),
            [(
                "order = [\"b\", \"a\"]".to_owned(),
                "order = [\"a\", \"b\", \"c\"]".to_owned()
            )],
            "{after}"
        );
    }

    #[test]
    fn set_order_adds_after_name() {
        let text = "name = \"Users\"\n\n[headers]\nAccept = \"x\"\n";

        let after = set_order(text, path(), &["a".to_owned()]).unwrap();

        assert_eq!(
            after,
            "name = \"Users\"\norder = [\"a\"]\n\n[headers]\nAccept = \"x\"\n"
        );
        assert_eq!(
            set_order("", path(), &["a".to_owned()]).unwrap(),
            "order = [\"a\"]\n",
            "new file"
        );
    }

    #[test]
    fn rename_in_order_and_remove_from_order() {
        let renamed = rename_in_order(FOLDER, path(), "a", "zed")
            .unwrap()
            .unwrap();
        let removed = remove_from_order(FOLDER, path(), "b").unwrap().unwrap();

        assert_eq!(
            changed(FOLDER, &renamed),
            [(
                "order = [\"b\", \"a\"]".to_owned(),
                "order = [\"b\", \"zed\"]".to_owned()
            )]
        );
        assert_eq!(
            changed(FOLDER, &removed),
            [(
                "order = [\"b\", \"a\"]".to_owned(),
                "order = [\"a\"]".to_owned()
            )]
        );
        assert_eq!(
            rename_in_order(FOLDER, path(), "nope", "x").unwrap(),
            None,
            "not listed"
        );
        assert_eq!(
            remove_from_order("name = \"U\"\n", path(), "a").unwrap(),
            None,
            "no order"
        );
    }

    #[test]
    fn new_files_parse() {
        let name = "A \"q\" ü";
        let request = Request::parse(&new_request(name), path(), &|_| None).unwrap();
        let manifest = Manifest::parse(&new_manifest(name), Path::new("pawsman.toml")).unwrap();
        let collection = Collection::from_files(
            Path::new("/c"),
            vec![
                SourceFile::new("pawsman.toml".into(), new_manifest("C").into_bytes()),
                SourceFile::new("f/_folder.toml".into(), new_folder(name).into_bytes()),
            ],
        )
        .unwrap();

        assert_eq!(
            (request.name(), request.method(), request.url()),
            (name, "GET", "")
        );
        assert_eq!(manifest.name(), name, "manifest");
        let [Node::Folder(folder)] = collection.root_folder().children() else {
            panic!("{:?}", collection.root_folder().children());
        };
        assert_eq!(folder.name(), name, "folder");
    }

    #[rstest]
    #[case("Create user", "create-user")]
    #[case("../../etc", "etc")]
    #[case("a/b", "a-b")]
    #[case(".hidden", "hidden")]
    #[case("  ", "")]
    #[case("Пользователи", "пользователи")]
    #[case("get_user -- v2", "get_user-v2")]
    #[case("a\u{0}b\nc", "a-b-c")]
    fn slug_cases(#[case] name: &str, #[case] expected: &str) {
        assert_eq!(slug(name), expected, "{name:?}");
    }

    #[test]
    fn slugs_are_capped_at_64_characters() {
        assert_eq!(slug(&"x".repeat(100)), "x".repeat(64));
        assert_eq!(
            slug(&format!("{}-y", "x".repeat(63))),
            "x".repeat(63),
            "no trailing dash after the cut"
        );
    }

    #[test]
    fn broken_toml_is_invalid_file() {
        for result in [
            set_name("name = ", path(), "x").map(|_| ()),
            set_order("[", path(), &[]).map(|_| ()),
            rename_in_order("[", path(), "a", "b").map(|_| ()),
            remove_from_order("[", path(), "a").map(|_| ()),
        ] {
            assert_eq!(
                result.unwrap_err().code(),
                crate::codes::INVALID_FILE,
                "code"
            );
        }
    }
}
