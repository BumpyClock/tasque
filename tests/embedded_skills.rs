mod common;

use common::{make_repo, tsq_bin};
use std::fs;
use std::process::Command;

#[test]
fn embedded_skills_fallback_installs_skill_when_disk_sources_missing() {
    let repo = make_repo();
    let repo_path = repo.path();

    let missing_skills = repo_path.join("missing-skills");
    let codex_home = repo_path.join(".codex");

    let output = Command::new(tsq_bin())
        .args(["init", "--install-skill", "--no-wizard"])
        .current_dir(repo_path)
        .env("TSQ_ACTOR", "rust-test")
        .env("TSQ_SKILLS_DIR", &missing_skills)
        .env("CODEX_HOME", &codex_home)
        .env("HOME", repo_path)
        .env("USERPROFILE", repo_path)
        .output()
        .expect("failed executing tsq binary");

    assert!(
        output.status.success(),
        "expected init install to succeed via embedded skills fallback\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let skill_marker = codex_home.join("skills").join("tasque").join("SKILL.md");
    assert!(
        skill_marker.exists(),
        "expected embedded skill to be installed at {}\nstdout:\n{}\nstderr:\n{}",
        skill_marker.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = fs::read_to_string(&skill_marker).expect("read installed embedded skill");
    assert!(contents.contains("tsq find ready --lane coding"));
    assert!(contents.contains("tsq create --parent <parent-id> --from-file tasks.md"));
    assert!(contents.contains("tsq spec <id> --show"));
}
