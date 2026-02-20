mod common;

use common::make_repo;
use std::path::PathBuf;
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
}

fn tsq_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsq") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return candidate;
        }
    }

    let binary_name = if cfg!(windows) { "tsq.exe" } else { "tsq" };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = manifest_dir.join("Cargo.toml");
    let candidate = manifest_dir.join("target").join("debug").join(binary_name);

    if candidate.exists() {
        return candidate;
    }

    let build_status = Command::new("cargo")
        .args([
            "build",
            "--quiet",
            "--manifest-path",
            manifest_path.to_string_lossy().as_ref(),
            "--bin",
            "tsq",
        ])
        .current_dir(&manifest_dir)
        .status()
        .expect("failed to invoke cargo build for tsq test binary");
    assert!(
        build_status.success(),
        "cargo build --bin tsq failed with status: {:?}",
        build_status.code()
    );

    candidate
}
