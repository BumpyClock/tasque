use super::*;
use std::collections::HashMap;

fn make_source(dir: &std::path::Path, files: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    for (name, content) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }
}

fn managed_skill_md() -> Vec<(&'static str, &'static str)> {
    vec![("SKILL.md", "# Tasque Skill\ntsq-managed-skill:v1\n")]
}

fn source_with_nested() -> Vec<(&'static str, &'static str)> {
    vec![
        ("SKILL.md", "# Tasque Skill\ntsq-managed-skill:v1\n"),
        ("sub/deep.txt", "nested content"),
    ]
}

fn refresh_options<'a>(target_dir: &'a str, source_dir: &'a str) -> SkillOperationOptions {
    let mut overrides = HashMap::new();
    overrides.insert(SkillTarget::Claude, target_dir.to_string());
    SkillOperationOptions {
        action: SkillAction::Refresh,
        skill_name: "test-skill".to_string(),
        targets: vec![SkillTarget::Claude],
        force: false,
        source_root_dir: Some(source_dir.to_string()),
        home_dir: Some(
            std::env::temp_dir()
                .join("tsq-unused-home")
                .display()
                .to_string(),
        ),
        codex_home: None,
        target_dir_overrides: Some(overrides),
    }
}

#[test]
fn refresh_missing_target_returns_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source").join("test-skill");
    make_source(&source, &managed_skill_md());

    let opts = refresh_options(
        &tmp.path().join("targets").display().to_string(),
        &tmp.path().join("source").display().to_string(),
    );
    let result = apply_skill_operation(opts).unwrap();
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].status, SkillResultStatus::NotFound);
    // No directory created
    assert!(!tmp.path().join("targets").join("test-skill").exists());
}

#[test]
fn refresh_file_target_returns_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source").join("test-skill");
    make_source(&source, &managed_skill_md());

    let targets = tmp.path().join("targets");
    fs::create_dir_all(&targets).unwrap();
    // Place a file where the skill dir would be
    fs::write(targets.join("test-skill"), "not a dir").unwrap();

    let opts = refresh_options(
        &targets.display().to_string(),
        &tmp.path().join("source").display().to_string(),
    );
    let result = apply_skill_operation(opts).unwrap();
    assert_eq!(result.results[0].status, SkillResultStatus::Skipped);
    // File preserved
    assert!(targets.join("test-skill").is_file());
    assert_eq!(
        fs::read_to_string(targets.join("test-skill")).unwrap(),
        "not a dir"
    );
}

#[test]
fn refresh_non_managed_dir_returns_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source").join("test-skill");
    make_source(&source, &managed_skill_md());

    let targets = tmp.path().join("targets");
    let skill_dir = targets.join("test-skill");
    make_source(
        &skill_dir,
        &[("SKILL.md", "# User skill\nno marker here\n")],
    );

    let opts = refresh_options(
        &targets.display().to_string(),
        &tmp.path().join("source").display().to_string(),
    );
    let result = apply_skill_operation(opts).unwrap();
    assert_eq!(result.results[0].status, SkillResultStatus::Skipped);
    // Dir preserved with original content
    assert_eq!(
        fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
        "# User skill\nno marker here\n"
    );
}

#[test]
fn refresh_managed_skillmd_only_marker_updates() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source").join("test-skill");
    make_source(&source, &source_with_nested());

    let targets = tmp.path().join("targets");
    let skill_dir = targets.join("test-skill");
    // Old managed install with only SKILL.md (no README.md)
    make_source(&skill_dir, &managed_skill_md());

    let opts = refresh_options(
        &targets.display().to_string(),
        &tmp.path().join("source").display().to_string(),
    );
    let result = apply_skill_operation(opts).unwrap();
    assert_eq!(result.results[0].status, SkillResultStatus::Updated);
    // New nested file copied in
    assert!(skill_dir.join("sub/deep.txt").exists());
    assert_eq!(
        fs::read_to_string(skill_dir.join("sub/deep.txt")).unwrap(),
        "nested content"
    );
    // Directory was replaced; SKILL.md is still present from source.
    assert!(skill_dir.join("SKILL.md").exists());
}

#[test]
fn is_managed_skill_only_checks_skill_md() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("skill");

    // No files at all
    fs::create_dir_all(&dir).unwrap();
    assert!(!is_managed_skill(&dir).unwrap());

    // Only SKILL.md with marker (no README.md)
    fs::write(dir.join("SKILL.md"), "tsq-managed-skill:v1").unwrap();
    assert!(is_managed_skill(&dir).unwrap());

    // SKILL.md without marker
    fs::write(dir.join("SKILL.md"), "just a skill").unwrap();
    assert!(!is_managed_skill(&dir).unwrap());
}

/// Test 1: Refresh with missing disk source falls back to embedded "tasque" skill,
/// updates existing managed target, and embedded content (markers) appear.
#[test]
fn refresh_embedded_fallback_updates_managed_target() {
    let tmp = tempfile::tempdir().unwrap();

    // Point source_root_dir at a missing path so disk source resolution fails.
    let missing_source = tmp.path().join("no-such-source");

    // Create an existing managed skill target (old content).
    let targets = tmp.path().join("targets");
    let skill_dir = targets.join("tasque");
    make_source(
        &skill_dir,
        &[("SKILL.md", "# Old\ntsq-managed-skill:v1\nold content")],
    );

    let mut overrides = HashMap::new();
    overrides.insert(SkillTarget::Claude, targets.display().to_string());
    let opts = SkillOperationOptions {
        action: SkillAction::Refresh,
        skill_name: "tasque".to_string(),
        targets: vec![SkillTarget::Claude],
        force: false,
        source_root_dir: Some(missing_source.display().to_string()),
        home_dir: Some(
            std::env::temp_dir()
                .join("tsq-unused-home")
                .display()
                .to_string(),
        ),
        codex_home: None,
        target_dir_overrides: Some(overrides),
    };

    let result = apply_skill_operation(opts).unwrap();
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].status, SkillResultStatus::Updated);

    // Target should now contain embedded skill content.
    let skill_md = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
    assert!(skill_md.contains("tsq find ready --lane coding"));
    assert!(skill_md.contains("tsq-managed-skill:v1"));
    // Old content should be gone (replaced, not merged).
    assert!(!skill_md.contains("old content"));
    // Embedded references should also be present.
    assert!(skill_dir.join("references").is_dir());
}

/// Test 2: Copy failure during refresh preserves existing managed target files.
/// Uses a symlink in source directory which copy_directory_recursive rejects.
#[cfg(unix)]
#[test]
fn refresh_copy_failure_preserves_target() {
    let tmp = tempfile::tempdir().unwrap();

    // Source with a FIFO (unsupported entry type for copy_directory_recursive).
    let source = tmp.path().join("source").join("test-skill");
    make_source(&source, &managed_skill_md());
    let fifo_path = source.join("bad-fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .expect("mkfifo should run");
    assert!(status.success(), "mkfifo failed: {:?}", status.code());

    // Existing managed target with known content.
    let targets = tmp.path().join("targets");
    let skill_dir = targets.join("test-skill");
    make_source(
        &skill_dir,
        &[
            ("SKILL.md", "# Managed\ntsq-managed-skill:v1\noriginal"),
            ("extra.txt", "preserve me"),
        ],
    );

    let opts = refresh_options(
        &targets.display().to_string(),
        &tmp.path().join("source").display().to_string(),
    );

    let err = apply_skill_operation(opts).unwrap_err();
    assert_eq!(err.code, "IO_ERROR");
    assert!(err.message.contains("unsupported entry"));

    // Target files must remain unchanged.
    let md = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
    assert!(md.contains("original"));
    assert_eq!(
        fs::read_to_string(skill_dir.join("extra.txt")).unwrap(),
        "preserve me"
    );
}

/// Test 3: SkillAction::Refresh serializes to "refresh" and deserializes back.
#[test]
fn skill_action_refresh_serialization_roundtrip() {
    let json = serde_json::to_string(&SkillAction::Refresh).unwrap();
    assert_eq!(json, "\"refresh\"");

    let deserialized: SkillAction = serde_json::from_str("\"refresh\"").unwrap();
    assert_eq!(deserialized, SkillAction::Refresh);
}
