//! Service-level skills-refresh tests (all platforms).
//! Exercises `TasqueService::skills_refresh` directly with explicit temp dirs,
//! avoiding env-override HOME tricks that are fragile on Windows.

use std::fs;
use tempfile::Builder;

use tasque::app::service::TasqueService;
use tasque::app::service_types::SkillsRefreshInput;
use tasque::skills::types::SkillResultStatus;

/// Build a TasqueService with a dummy repo root (skills refresh
/// does not need a real .tasque repo).
fn make_service(repo_root: &std::path::Path) -> TasqueService {
    TasqueService::new(repo_root.display().to_string(), "rust-test", || {
        "2025-01-01T00:00:00Z".to_string()
    })
}

/// Extract status string for a target from SkillOperationSummary JSON.
fn json_status_for(json: &serde_json::Value, target: &str) -> String {
    json.get("results")
        .and_then(|r| r.as_array())
        .expect("results array")
        .iter()
        .find(|r| r.get("target").and_then(|v| v.as_str()) == Some(target))
        .unwrap_or_else(|| panic!("no result for target '{}'", target))
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no status for target '{}'", target))
        .to_string()
}

/// Missing targets → not_found, no .tasque created,
/// JSON serialization uses snake_case statuses.
#[test]
fn service_refresh_missing_targets_not_found_and_no_repo_state() {
    let tmp = Builder::new()
        .prefix("tsq-svc-refresh-notfound-")
        .tempdir()
        .expect("tempdir");
    let t = tmp.path();

    let repo_root = t.join("repo");
    fs::create_dir_all(&repo_root).expect("repo dir");

    let source_root = t.join("skills-source");
    let skill_dir = source_root.join("tasque");
    fs::create_dir_all(&skill_dir).expect("skill src dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# Tasque Skill\n",
    )
    .expect("write SKILL.md");

    let home = t.join("home");
    fs::create_dir_all(&home).expect("home");
    let codex_home = t.join("codex");
    fs::create_dir_all(&codex_home).expect("codex home");

    let svc = make_service(&repo_root);
    let summary = svc
        .skills_refresh(SkillsRefreshInput {
            source_root_dir: Some(source_root.display().to_string()),
            home_dir: Some(home.display().to_string()),
            codex_home: Some(codex_home.display().to_string()),
        })
        .expect("refresh should succeed");

    // Verify all 4 targets present
    assert_eq!(summary.results.len(), 4);
    for r in &summary.results {
        assert_eq!(
            r.status,
            SkillResultStatus::NotFound,
            "{:?} should be not_found",
            r.target
        );
    }

    // JSON serialization uses snake_case
    let json = serde_json::to_value(&summary).expect("serialize");
    for target in ["claude", "codex", "copilot", "opencode"] {
        assert_eq!(json_status_for(&json, target), "not_found");
    }

    // No .tasque created in repo root
    assert!(!repo_root.join(".tasque").exists());
}

/// Non-managed dir/file → skipped and managed target → updated.
#[test]
fn service_refresh_skip_update_cases() {
    let tmp = Builder::new()
        .prefix("tsq-svc-refresh-mixed-")
        .tempdir()
        .expect("tempdir");
    let t = tmp.path();

    let repo_root = t.join("repo");
    fs::create_dir_all(&repo_root).expect("repo dir");

    let source_root = t.join("skills-source");
    let source_skill = source_root.join("tasque");
    fs::create_dir_all(&source_skill).expect("src skill dir");
    fs::write(
        source_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# source-v2 content\n",
    )
    .expect("write source SKILL.md");

    let home = t.join("home");
    fs::create_dir_all(&home).expect("home");
    let codex_home = t.join("codex");
    fs::create_dir_all(&codex_home).expect("codex home");

    let claude_skill = home.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("claude skill dir");
    fs::write(
        claude_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# old-v1\n",
    )
    .expect("write old SKILL.md");

    let codex_skills = codex_home.join("skills");
    fs::create_dir_all(&codex_skills).expect("codex skills dir");
    fs::write(codex_skills.join("tasque"), "file content").expect("codex file");

    let opencode_skill = home.join(".opencode").join("skills").join("tasque");
    fs::create_dir_all(&opencode_skill).expect("opencode skill dir");
    fs::write(opencode_skill.join("local.txt"), "user local content").expect("opencode local.txt");

    let svc = make_service(&repo_root);
    let summary = svc
        .skills_refresh(SkillsRefreshInput {
            source_root_dir: Some(source_root.display().to_string()),
            home_dir: Some(home.display().to_string()),
            codex_home: Some(codex_home.display().to_string()),
        })
        .expect("refresh should succeed");

    let json = serde_json::to_value(&summary).expect("serialize");
    assert_eq!(json_status_for(&json, "claude"), "updated");
    assert_eq!(json_status_for(&json, "codex"), "skipped");
    assert_eq!(json_status_for(&json, "copilot"), "not_found");
    assert_eq!(json_status_for(&json, "opencode"), "skipped");

    let skill_md = fs::read_to_string(claude_skill.join("SKILL.md")).expect("read");
    assert!(skill_md.contains("source-v2 content"));
    assert!(!skill_md.contains("old-v1"));
    assert_eq!(
        fs::read_to_string(codex_skills.join("tasque")).unwrap(),
        "file content"
    );
    assert_eq!(
        fs::read_to_string(opencode_skill.join("local.txt")).unwrap(),
        "user local content"
    );
    assert!(!repo_root.join(".tasque").exists());
}

/// Embedded fallback works when disk source lookup misses.
#[test]
fn service_refresh_embedded_fallback_when_source_missing() {
    let tmp = Builder::new()
        .prefix("tsq-svc-refresh-embedded-")
        .tempdir()
        .expect("tempdir");
    let t = tmp.path();

    let repo_root = t.join("repo");
    fs::create_dir_all(&repo_root).expect("repo dir");
    let home = t.join("home");
    fs::create_dir_all(&home).expect("home");
    let codex_home = t.join("codex");
    fs::create_dir_all(&codex_home).expect("codex home");

    let copilot_skill = home.join(".copilot").join("skills").join("tasque");
    fs::create_dir_all(&copilot_skill).expect("copilot skill dir");
    fs::write(
        copilot_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# xyzzy-old-placeholder\n",
    )
    .expect("write copilot SKILL.md");

    let empty_source = t.join("empty-source");
    fs::create_dir_all(&empty_source).expect("empty source");

    let svc = make_service(&repo_root);
    let summary = svc
        .skills_refresh(SkillsRefreshInput {
            source_root_dir: Some(empty_source.display().to_string()),
            home_dir: Some(home.display().to_string()),
            codex_home: Some(codex_home.display().to_string()),
        })
        .expect("refresh with embedded fallback");

    let json = serde_json::to_value(&summary).expect("serialize");
    assert_eq!(json_status_for(&json, "copilot"), "updated");

    let copilot_md = fs::read_to_string(copilot_skill.join("SKILL.md")).expect("read");
    assert!(copilot_md.contains("tsq-managed-skill:v1"));
    assert!(!copilot_md.contains("xyzzy-old-placeholder"));
    assert!(!repo_root.join(".tasque").exists());
}

#[cfg(unix)]
#[test]
fn service_refresh_propagates_source_copy_error() {
    let tmp = Builder::new()
        .prefix("tsq-svc-refresh-error-")
        .tempdir()
        .expect("tempdir");
    let t = tmp.path();

    let repo_root = t.join("repo");
    fs::create_dir_all(&repo_root).expect("repo dir");
    let home = t.join("home");
    fs::create_dir_all(&home).expect("home");
    let codex_home = t.join("codex");
    fs::create_dir_all(&codex_home).expect("codex home");

    let source_skill = t.join("skills-source").join("tasque");
    fs::create_dir_all(&source_skill).expect("src skill dir");
    fs::write(
        source_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# source-v2 content\n",
    )
    .expect("write source SKILL.md");
    let fifo_path = source_skill.join("bad-fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .expect("mkfifo should run");
    assert!(status.success(), "mkfifo failed: {:?}", status.code());

    let claude_skill = home.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("claude skill dir");
    fs::write(
        claude_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# old-v1\n",
    )
    .expect("write old SKILL.md");

    let svc = make_service(&repo_root);
    let err = svc
        .skills_refresh(SkillsRefreshInput {
            source_root_dir: Some(t.join("skills-source").display().to_string()),
            home_dir: Some(home.display().to_string()),
            codex_home: Some(codex_home.display().to_string()),
        })
        .expect_err("unsupported source entry should propagate as error");

    assert_eq!(err.code, "IO_ERROR");
    assert!(err.message.contains("unsupported entry"));
    let skill_md = fs::read_to_string(claude_skill.join("SKILL.md")).expect("read");
    assert!(skill_md.contains("old-v1"));
}
