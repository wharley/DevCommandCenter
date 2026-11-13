//! The paths a pull request changed, for the hub's summary: a comment or
//! a review that names one of them shows it as a chip that opens the file.
//! Read from the fields the summary already asks its forge for, never from
//! the patches.

use serde_json::Value;

/// `files[].path` of `gh pr view --json files`.
pub fn changed_paths(pull_request: &Value) -> Vec<String> {
    pull_request
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| file.get("path").and_then(Value::as_str))
        .filter(|path| !path.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_paths_come_from_the_pull_requests_own_file_list() {
        let raw = json!({ "files": [
            { "path": "src/a.rs", "additions": 1, "deletions": 0 },
            { "path": "" },
            { "additions": 3 },
            { "path": "README.md" },
        ]});
        assert_eq!(changed_paths(&raw), ["src/a.rs", "README.md"]);
        assert!(changed_paths(&json!({})).is_empty());
    }
}
