//! CLI-level skills-refresh tests (non-Windows).
//! Exercises `tsq skills refresh` via subprocess with env overrides.
//! Service-level (all-platform) tests live in skills_refresh_service.rs.

#[cfg(not(target_os = "windows"))]
use std::fs;
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;
#[cfg(not(target_os = "windows"))]
use std::process::Command;
#[cfg(not(target_os = "windows"))]
use tempfile::Builder;

#[cfg(not(target_os = "windows"))]
fn tsq_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsq") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return candidate;
        }
    }

    let binary_name = "tsq";
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

#[cfg(not(target_os = "windows"))]
/// Parse JSON envelope from `tsq skills refresh --json` stdout.
/// Returns (envelope, results_array).
fn parse_refresh_envelope(stdout: &str) -> (serde_json::Value, Vec<serde_json::Value>) {
    let envelope: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("expected valid JSON envelope");
    assert_eq!(envelope.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        envelope.get("command").and_then(|v| v.as_str()),
        Some("tsq skills refresh")
    );
    let data = envelope.get("data").expect("envelope must have data");
    assert_eq!(data.get("action").and_then(|v| v.as_str()), Some("refresh"));
    assert_eq!(
        data.get("skill_name").and_then(|v| v.as_str()),
        Some("tasque")
    );
    let results = data
        .get("results")
        .and_then(|v| v.as_array())
        .expect("data.results must be an array")
        .clone();
    assert_eq!(results.len(), 4, "expected 4 target results");
    (envelope, results)
}

#[cfg(not(target_os = "windows"))]
/// Find status string for a given target name in results array.
fn status_for(results: &[serde_json::Value], target: &str) -> String {
    results
        .iter()
        .find(|r| r.get("target").and_then(|v| v.as_str()) == Some(target))
        .unwrap_or_else(|| panic!("no result for target '{}'", target))
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no status for target '{}'", target))
        .to_string()
}

#[cfg(not(target_os = "windows"))]
#[test]
fn skills_refresh_json_works_without_tasque_and_creates_no_repo_state() {
    let temp_root = Builder::new()
        .prefix("tsq-skills-refresh-test-")
        .tempdir()
        .expect("failed creating temp dir");

    let temp_path = temp_root.path();

    // Source root has a valid managed marker; missing targets still stay not_found.
    let source_root = temp_path.join("skills-source");
    let skill_dir = source_root.join("tasque");
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# Tasque Skill\n",
    )
    .expect("write SKILL.md");

    let home_dir = temp_path.join("home");
    fs::create_dir_all(&home_dir).expect("create home dir");

    let codex_home = temp_path.join("codex-home");
    fs::create_dir_all(&codex_home).expect("create codex home dir");

    let cwd = temp_path.join("cwd");
    fs::create_dir_all(&cwd).expect("create cwd dir");

    let output = Command::new(tsq_bin())
        .args(["skills", "refresh", "--json"])
        .current_dir(&cwd)
        .env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("CODEX_HOME", &codex_home)
        .env("TSQ_SKILLS_DIR", &source_root)
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

    let (_envelope, results) = parse_refresh_envelope(&stdout);

    for target in ["claude", "codex", "copilot", "opencode"] {
        assert_eq!(
            status_for(&results, target),
            "not_found",
            "{} should be not_found",
            target
        );
    }

    // No .tasque created in cwd
    assert!(
        !cwd.join(".tasque").exists(),
        "skills refresh must not create .tasque in cwd"
    );

    // No default target dirs created
    assert!(!home_dir.join(".claude").exists());
    assert!(!codex_home.join("skills").exists());
    assert!(!home_dir.join(".copilot").exists());
    assert!(!home_dir.join(".opencode").exists());
}

#[cfg(not(target_os = "windows"))]
#[test]
fn skills_refresh_preserves_non_managed_directory_and_file() {
    let temp_root = Builder::new()
        .prefix("tsq-refresh-preserve-")
        .tempdir()
        .expect("create temp dir");
    let temp_path = temp_root.path();

    // Source root with managed marker + source-v2 content
    let source_root = temp_path.join("skills-source");
    let source_skill = source_root.join("tasque");
    fs::create_dir_all(&source_skill).expect("create source skill dir");
    fs::write(
        source_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# source-v2\n",
    )
    .expect("write source SKILL.md");

    let home_dir = temp_path.join("home");
    fs::create_dir_all(&home_dir).expect("create home");

    let codex_home = temp_path.join("codex-home");
    fs::create_dir_all(&codex_home).expect("create codex home");

    // Claude target: non-managed directory with local.txt (no marker in SKILL.md)
    let claude_skill = home_dir.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("create claude skill dir");
    fs::write(claude_skill.join("local.txt"), "user content").expect("write local.txt");
    // No SKILL.md with marker → not managed

    // Codex target: a plain file instead of a directory
    let codex_skills = codex_home.join("skills");
    fs::create_dir_all(&codex_skills).expect("create codex skills dir");
    let codex_skill_path = codex_skills.join("tasque");
    fs::write(&codex_skill_path, "codex file content").expect("write codex file");

    // Copilot/Opencode: don't exist

    let cwd = temp_path.join("cwd");
    fs::create_dir_all(&cwd).expect("create cwd");

    let output = Command::new(tsq_bin())
        .args(["skills", "refresh", "--json"])
        .current_dir(&cwd)
        .env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("CODEX_HOME", &codex_home)
        .env("TSQ_SKILLS_DIR", &source_root)
        .output()
        .expect("run tsq");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let (_envelope, results) = parse_refresh_envelope(&stdout);

    // Claude: skipped (non-managed dir)
    assert_eq!(status_for(&results, "claude"), "skipped");
    // Codex: skipped (file, not managed dir)
    assert_eq!(status_for(&results, "codex"), "skipped");
    // Copilot/Opencode: not found
    assert_eq!(status_for(&results, "copilot"), "not_found");
    assert_eq!(status_for(&results, "opencode"), "not_found");

    // Claude local.txt preserved
    assert!(
        claude_skill.join("local.txt").exists(),
        "local.txt must survive refresh skip"
    );
    assert_eq!(
        fs::read_to_string(claude_skill.join("local.txt")).unwrap(),
        "user content"
    );

    // Codex file preserved
    assert!(
        codex_skill_path.is_file(),
        "codex file must survive refresh skip"
    );
    assert_eq!(
        fs::read_to_string(&codex_skill_path).unwrap(),
        "codex file content"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn skills_refresh_updates_managed_skill_with_skill_md_marker_only() {
    let temp_root = Builder::new()
        .prefix("tsq-refresh-update-")
        .tempdir()
        .expect("create temp dir");
    let temp_path = temp_root.path();

    // Source root: marker + source-v2 content + nested file
    let source_root = temp_path.join("skills-source");
    let source_skill = source_root.join("tasque");
    let source_nested = source_skill.join("nested");
    fs::create_dir_all(&source_nested).expect("create nested source dir");
    fs::write(
        source_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# source-v2 content\n",
    )
    .expect("write source SKILL.md");
    fs::write(source_nested.join("new.txt"), "nested content").expect("write nested/new.txt");

    let home_dir = temp_path.join("home");
    fs::create_dir_all(&home_dir).expect("create home");

    let codex_home = temp_path.join("codex-home");
    fs::create_dir_all(&codex_home).expect("create codex home");

    // Claude target: managed (SKILL.md with marker), old content, stale file
    let claude_skill = home_dir.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("create claude skill dir");
    fs::write(
        claude_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# old-v1 content\n",
    )
    .expect("write old SKILL.md");
    fs::write(claude_skill.join("old-local.txt"), "stale").expect("write old-local.txt");
    // No README — marker-only detection

    let cwd = temp_path.join("cwd");
    fs::create_dir_all(&cwd).expect("create cwd");

    let output = Command::new(tsq_bin())
        .args(["skills", "refresh", "--json"])
        .current_dir(&cwd)
        .env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("CODEX_HOME", &codex_home)
        .env("TSQ_SKILLS_DIR", &source_root)
        .output()
        .expect("run tsq");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let (_envelope, results) = parse_refresh_envelope(&stdout);

    // Claude: updated
    assert_eq!(status_for(&results, "claude"), "updated");

    // SKILL.md has new source-v2 content
    let skill_md = fs::read_to_string(claude_skill.join("SKILL.md")).expect("read SKILL.md");
    assert!(
        skill_md.contains("source-v2 content"),
        "SKILL.md must contain source-v2 content, got: {}",
        skill_md
    );
    assert!(
        !skill_md.contains("old-v1"),
        "SKILL.md must not contain old-v1 content"
    );

    // Nested file present
    assert!(
        claude_skill.join("nested").join("new.txt").exists(),
        "nested/new.txt must exist after refresh"
    );
    assert_eq!(
        fs::read_to_string(claude_skill.join("nested").join("new.txt")).unwrap(),
        "nested content"
    );

    // Old stale file removed (entire dir replaced)
    assert!(
        !claude_skill.join("old-local.txt").exists(),
        "old-local.txt must be removed after refresh"
    );

    // No .tasque created in cwd
    assert!(
        !cwd.join(".tasque").exists(),
        "skills refresh must not create .tasque in cwd"
    );

    // Other targets not_found
    assert_eq!(status_for(&results, "codex"), "not_found");
    assert_eq!(status_for(&results, "copilot"), "not_found");
    assert_eq!(status_for(&results, "opencode"), "not_found");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn skills_refresh_uses_embedded_skill_fallback_when_disk_source_missing() {
    let temp_root = Builder::new()
        .prefix("tsq-refresh-embedded-")
        .tempdir()
        .expect("create temp dir");
    let temp_path = temp_root.path();

    let home_dir = temp_path.join("home");
    fs::create_dir_all(&home_dir).expect("create home");

    let codex_home = temp_path.join("codex-home");
    fs::create_dir_all(&codex_home).expect("create codex home");

    // Claude target: managed with old content
    let claude_skill = home_dir.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("create claude skill dir");
    fs::write(
        claude_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# old-v1 placeholder\n",
    )
    .expect("write old SKILL.md");

    // cwd: empty temp dir, no SKILLS/ subdir
    let cwd = temp_path.join("cwd");
    fs::create_dir_all(&cwd).expect("create cwd");

    // TSQ_SKILLS_DIR: empty dir (no tasque/ subdir → source lookup misses)
    let empty_source = temp_path.join("empty-source");
    fs::create_dir_all(&empty_source).expect("create empty source");

    let output = Command::new(tsq_bin())
        .args(["skills", "refresh", "--json"])
        .current_dir(&cwd)
        .env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("CODEX_HOME", &codex_home)
        .env("TSQ_SKILLS_DIR", &empty_source)
        .output()
        .expect("run tsq");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success with embedded fallback\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    let (_envelope, results) = parse_refresh_envelope(&stdout);

    // Claude: updated from embedded source
    assert_eq!(status_for(&results, "claude"), "updated");

    // SKILL.md should contain embedded content
    let skill_md = fs::read_to_string(claude_skill.join("SKILL.md")).expect("read SKILL.md");
    assert!(
        skill_md.contains("tsq-managed-skill:v1"),
        "SKILL.md must contain managed marker after embedded refresh"
    );
    assert!(
        skill_md.contains("tsq find ready --lane coding"),
        "SKILL.md must contain embedded skill text, got: {}...",
        &skill_md[..skill_md.len().min(200)]
    );
    assert!(
        !skill_md.contains("old-v1 placeholder"),
        "SKILL.md must not contain old content"
    );

    // Other targets not_found
    assert_eq!(status_for(&results, "codex"), "not_found");
    assert_eq!(status_for(&results, "copilot"), "not_found");
    assert_eq!(status_for(&results, "opencode"), "not_found");

    // No .tasque created
    assert!(
        !cwd.join(".tasque").exists(),
        "skills refresh must not create .tasque in cwd"
    );
}
