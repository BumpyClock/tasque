//! CLI-level skills-refresh smoke tests (non-Windows).
//! Service-level semantic matrix lives in skills_refresh_service.rs.

#[cfg(not(target_os = "windows"))]
mod common;

#[cfg(not(target_os = "windows"))]
use common::tsq_bin;
#[cfg(not(target_os = "windows"))]
use std::fs;
#[cfg(not(target_os = "windows"))]
use std::process::Command;
#[cfg(not(target_os = "windows"))]
use tempfile::{Builder, TempDir};

#[cfg(not(target_os = "windows"))]
struct RefreshCliFixture {
    _temp_root: TempDir,
    cwd: std::path::PathBuf,
    home_dir: std::path::PathBuf,
    codex_home: std::path::PathBuf,
    source_root: std::path::PathBuf,
}

#[cfg(not(target_os = "windows"))]
impl RefreshCliFixture {
    fn new(prefix: &str) -> Self {
        let temp_root = Builder::new().prefix(prefix).tempdir().expect("temp dir");
        let temp_path = temp_root.path();
        let source_root = temp_path.join("skills-source");
        let skill_dir = source_root.join("tasque");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "<!-- tsq-managed-skill:v1 -->\n# Tasque Skill\n",
        )
        .expect("write SKILL.md");
        let home_dir = temp_path.join("home");
        let codex_home = temp_path.join("codex-home");
        let cwd = temp_path.join("cwd");
        fs::create_dir_all(&home_dir).expect("create home dir");
        fs::create_dir_all(&codex_home).expect("create codex home dir");
        fs::create_dir_all(&cwd).expect("create cwd dir");
        Self {
            _temp_root: temp_root,
            cwd,
            home_dir,
            codex_home,
            source_root,
        }
    }

    fn run_refresh_json(&self) -> serde_json::Value {
        let output = Command::new(tsq_bin())
            .args(["skills", "refresh", "--json"])
            .current_dir(&self.cwd)
            .env("HOME", &self.home_dir)
            .env("USERPROFILE", &self.home_dir)
            .env("CODEX_HOME", &self.codex_home)
            .env("TSQ_SKILLS_DIR", &self.source_root)
            .output()
            .expect("failed executing tsq binary");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "expected success\nstdout:\n{}\nstderr:\n{}",
            stdout,
            stderr
        );
        serde_json::from_str(stdout.trim()).expect("expected valid JSON envelope")
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn skills_refresh_json_wires_env_and_creates_no_repo_state() {
    let fixture = RefreshCliFixture::new("tsq-skills-refresh-test-");

    let envelope = fixture.run_refresh_json();

    assert_eq!(envelope.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        envelope.get("command").and_then(|v| v.as_str()),
        Some("tsq skills refresh")
    );
    let data = envelope.get("data").expect("envelope data");
    assert_eq!(data.get("action").and_then(|v| v.as_str()), Some("refresh"));
    assert_eq!(
        data.get("skill_name").and_then(|v| v.as_str()),
        Some("tasque")
    );
    let results = data
        .get("results")
        .and_then(|v| v.as_array())
        .expect("results must be an array");
    assert!(!results.is_empty(), "results must not be empty");
    assert!(
        !fixture.cwd.join(".tasque").exists(),
        "skills refresh must not create .tasque in cwd"
    );
}

/// Regression test for the CWD-as-skill-source injection vector: a
/// `SKILLS/tasque/SKILL.md` sitting in the process's current working
/// directory must never be treated as a trusted refresh source, even when
/// `TSQ_SKILLS_DIR` is unset. Refresh must fall back to the trusted
/// exe-relative / embedded source instead of the untrusted CWD payload.
#[cfg(not(target_os = "windows"))]
#[test]
fn skills_refresh_ignores_untrusted_cwd_skills_source() {
    let temp_root = Builder::new()
        .prefix("tsq-skills-refresh-cwd-trust-test-")
        .tempdir()
        .expect("temp dir");
    let temp_path = temp_root.path();

    // Attacker-controlled CWD carrying a malicious managed-looking skill.
    let attacker_cwd = temp_path.join("attacker-cwd");
    let attacker_skill_dir = attacker_cwd.join("SKILLS").join("tasque");
    fs::create_dir_all(&attacker_skill_dir).expect("create attacker skill dir");
    fs::write(
        attacker_skill_dir.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# Malicious Tasque Skill\nINJECTED-BY-CWD-TEST\n",
    )
    .expect("write attacker SKILL.md");

    // A pre-existing managed install at the Claude target that refresh
    // should update from a trusted source only.
    let home_dir = temp_path.join("home");
    let codex_home = temp_path.join("codex-home");
    let claude_skill_dir = home_dir.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill_dir).expect("create claude skill dir");
    fs::write(
        claude_skill_dir.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# Old Tasque Skill\n",
    )
    .expect("write pre-existing managed SKILL.md");
    fs::create_dir_all(&codex_home).expect("create codex home dir");

    let output = Command::new(tsq_bin())
        .args(["skills", "refresh", "--json"])
        .current_dir(&attacker_cwd)
        .env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("CODEX_HOME", &codex_home)
        .env_remove("TSQ_SKILLS_DIR")
        .output()
        .expect("failed executing tsq binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let refreshed = fs::read_to_string(claude_skill_dir.join("SKILL.md"))
        .expect("expected SKILL.md to still exist after refresh");
    assert!(
        !refreshed.contains("INJECTED-BY-CWD-TEST"),
        "untrusted CWD SKILLS/ payload must never reach a managed target\nstdout:\n{}\nstderr:\n{}\nrefreshed SKILL.md:\n{}",
        stdout,
        stderr,
        refreshed
    );
}
