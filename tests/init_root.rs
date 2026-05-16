mod common;

use common::{make_repo, run_cli};
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-c")
        .arg("safe.bareRepository=all")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command failed");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:{}\nstderr:{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_in_git_subproject_under_ancestor_tasque_initializes_subproject_cwd() {
    let workspace = make_repo();
    let ancestor = workspace.path();

    let ancestor_init = run_cli(ancestor, ["init"]);
    assert_eq!(ancestor_init.code, 0, "stderr: {}", ancestor_init.stderr);
    let ancestor_config_path = ancestor.join(".tasque").join("config.json");
    assert!(ancestor_config_path.is_file());
    let ancestor_config_before =
        fs::read_to_string(&ancestor_config_path).expect("ancestor config");

    let subproject = ancestor.join("pi-tasque");
    fs::create_dir_all(&subproject).expect("create subproject");
    git(&subproject, &["init"]);
    git(&subproject, &["config", "user.name", "rust-test"]);
    git(
        &subproject,
        &["config", "user.email", "rust-test@example.com"],
    );

    let subproject_init = run_cli(&subproject, ["init"]);
    assert_eq!(
        subproject_init.code, 0,
        "expected nested init success\nstdout:\n{}\nstderr:\n{}",
        subproject_init.stdout, subproject_init.stderr
    );

    assert!(subproject.join(".tasque").join("config.json").is_file());
    assert!(subproject.join(".tasque").join("events.jsonl").is_file());
    assert!(subproject.join(".tasque").join(".gitignore").is_file());
    assert!(subproject.join(".tasque").join("snapshots").is_dir());
    assert_eq!(
        fs::read_to_string(&ancestor_config_path).expect("ancestor config after nested init"),
        ancestor_config_before
    );
    assert!(
        subproject_init
            .stdout
            .contains("created .tasque/config.json"),
        "expected init output to mention cwd .tasque config\nstdout:\n{}",
        subproject_init.stdout
    );
}
