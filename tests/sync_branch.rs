mod common;

use common::{make_repo, run_cli};
use serde_json::Value;
use std::fs;

#[test]
fn sync_branch_requires_git_repo() {
    let repo = make_repo();
    let root = repo.path();
    fs::create_dir_all(root.join(".tasque")).unwrap();
    let config = serde_json::json!({
        "schema_version": 1,
        "snapshot_every": 200,
        "sync_branch": "tasque-sync"
    });
    fs::write(
        root.join(".tasque").join("config.json"),
        format!("{}\n", config),
    )
    .unwrap();

    let result = run_cli(root, ["list", "--json"]);
    assert_eq!(result.code, 2);
    let envelope: Value = serde_json::from_str(result.stdout.trim()).unwrap();
    let code = envelope
        .get("error")
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str);
    assert_eq!(code, Some("GIT_NOT_AVAILABLE"));
}
