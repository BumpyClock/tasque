//! Service-level skills-refresh tests (all platforms).
//! Exercises `TasqueService::skills_refresh` directly with explicit temp dirs,
//! avoiding env-override HOME tricks that are fragile on Windows.

use std::fs;
use tempfile::{Builder, TempDir};

use tasque::app::service::TasqueService;
use tasque::app::service_types::SkillsRefreshInput;
use tasque::skills::types::SkillResultStatus;

struct RefreshFixture {
    _tmp: TempDir,
    repo_root: std::path::PathBuf,
    home: std::path::PathBuf,
    codex_home: std::path::PathBuf,
    source_root: std::path::PathBuf,
}

impl RefreshFixture {
    fn new(prefix: &str) -> Self {
        let tmp = Builder::new().prefix(prefix).tempdir().expect("tempdir");
        let t = tmp.path();
        let repo_root = t.join("repo");
        let home = t.join("home");
        let codex_home = t.join("codex");
        let source_root = t.join("skills-source");
        fs::create_dir_all(&repo_root).expect("repo dir");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&codex_home).expect("codex home");
        Self {
            _tmp: tmp,
            repo_root,
            home,
            codex_home,
            source_root,
        }
    }

    fn write_source_skill(&self, body: &str) -> std::path::PathBuf {
        let source_skill = self.source_root.join("tasque");
        fs::create_dir_all(&source_skill).expect("src skill dir");
        fs::write(source_skill.join("SKILL.md"), body).expect("write source SKILL.md");
        source_skill
    }

    fn service(&self) -> TasqueService {
        TasqueService::new(self.repo_root.display().to_string(), "rust-test", || {
            "2025-01-01T00:00:00Z".to_string()
        })
    }

    fn input_for(&self, source_root: &std::path::Path) -> SkillsRefreshInput {
        SkillsRefreshInput {
            source_root_dir: Some(source_root.display().to_string()),
            home_dir: Some(self.home.display().to_string()),
            codex_home: Some(self.codex_home.display().to_string()),
        }
    }

    fn refresh(&self) -> tasque::skills::types::SkillOperationSummary {
        self.service()
            .skills_refresh(self.input_for(&self.source_root))
            .expect("refresh should succeed")
    }
}

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

#[test]
fn service_refresh_missing_targets_not_found_and_no_repo_state() {
    let fixture = RefreshFixture::new("tsq-svc-refresh-notfound-");
    fixture.write_source_skill("<!-- tsq-managed-skill:v1 -->\n# Tasque Skill\n");

    let summary = fixture.refresh();

    assert_eq!(summary.results.len(), 4);
    for r in &summary.results {
        assert_eq!(
            r.status,
            SkillResultStatus::NotFound,
            "{:?} should be not_found",
            r.target
        );
    }
    let json = serde_json::to_value(&summary).expect("serialize");
    for target in ["claude", "codex", "copilot", "opencode"] {
        assert_eq!(json_status_for(&json, target), "not_found");
    }
    assert!(!fixture.repo_root.join(".tasque").exists());
}

#[test]
fn service_refresh_skip_update_cases() {
    let fixture = RefreshFixture::new("tsq-svc-refresh-mixed-");
    fixture.write_source_skill("<!-- tsq-managed-skill:v1 -->\n# source-v2 content\n");

    let claude_skill = fixture.home.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("claude skill dir");
    fs::write(
        claude_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# old-v1\n",
    )
    .expect("write old SKILL.md");
    let codex_skills = fixture.codex_home.join("skills");
    fs::create_dir_all(&codex_skills).expect("codex skills dir");
    fs::write(codex_skills.join("tasque"), "file content").expect("codex file");
    let opencode_skill = fixture.home.join(".opencode").join("skills").join("tasque");
    fs::create_dir_all(&opencode_skill).expect("opencode skill dir");
    fs::write(opencode_skill.join("local.txt"), "user local content").expect("opencode local.txt");

    let summary = fixture.refresh();

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
    assert!(!fixture.repo_root.join(".tasque").exists());
}

#[test]
fn service_refresh_embedded_fallback_when_source_missing() {
    let fixture = RefreshFixture::new("tsq-svc-refresh-embedded-");
    let copilot_skill = fixture.home.join(".copilot").join("skills").join("tasque");
    fs::create_dir_all(&copilot_skill).expect("copilot skill dir");
    fs::write(
        copilot_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# xyzzy-old-placeholder\n",
    )
    .expect("write copilot SKILL.md");
    let empty_source = fixture._tmp.path().join("empty-source");
    fs::create_dir_all(&empty_source).expect("empty source");

    let summary = fixture
        .service()
        .skills_refresh(fixture.input_for(&empty_source))
        .expect("refresh with embedded fallback");

    let json = serde_json::to_value(&summary).expect("serialize");
    assert_eq!(json_status_for(&json, "copilot"), "updated");
    let copilot_md = fs::read_to_string(copilot_skill.join("SKILL.md")).expect("read");
    assert!(copilot_md.contains("tsq-managed-skill:v1"));
    assert!(!copilot_md.contains("xyzzy-old-placeholder"));
    assert!(!fixture.repo_root.join(".tasque").exists());
}

#[cfg(unix)]
#[test]
fn service_refresh_propagates_source_copy_error() {
    let fixture = RefreshFixture::new("tsq-svc-refresh-error-");
    let source_skill =
        fixture.write_source_skill("<!-- tsq-managed-skill:v1 -->\n# source-v2 content\n");
    let fifo_path = source_skill.join("bad-fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .expect("mkfifo should run");
    assert!(status.success(), "mkfifo failed: {:?}", status.code());
    let claude_skill = fixture.home.join(".claude").join("skills").join("tasque");
    fs::create_dir_all(&claude_skill).expect("claude skill dir");
    fs::write(
        claude_skill.join("SKILL.md"),
        "<!-- tsq-managed-skill:v1 -->\n# old-v1\n",
    )
    .expect("write old SKILL.md");

    let err = fixture
        .service()
        .skills_refresh(fixture.input_for(&fixture.source_root))
        .expect_err("unsupported source entry should propagate as error");

    assert_eq!(err.code, "IO_ERROR");
    assert!(err.message.contains("unsupported entry"));
    let skill_md = fs::read_to_string(claude_skill.join("SKILL.md")).expect("read");
    assert!(skill_md.contains("old-v1"));
}
