use crate::errors::TsqError;
use rust_embed::RustEmbed;
use std::fs;
use std::path::{Path, PathBuf};
use ulid::Ulid;

#[derive(RustEmbed)]
#[folder = "SKILLS"]
struct EmbeddedSkills;

pub struct EmbeddedSkillMaterialization {
    pub skill_name: String,
    pub skill_root: PathBuf,
    pub temp_root: PathBuf,
}

pub fn materialize_embedded_skill(
    skill_name: &str,
) -> Result<EmbeddedSkillMaterialization, TsqError> {
    let temp_root = std::env::temp_dir().join(format!("tsq-embedded-{}", Ulid::new()));
    let skill_root = temp_root.join(skill_name);

    let mut found = false;
    let prefix = format!("{}/", skill_name);

    for asset_path in EmbeddedSkills::iter() {
        let asset_path = asset_path.as_ref();
        if !asset_path.starts_with(&prefix) {
            continue;
        }

        let relative = &asset_path[prefix.len()..];
        if relative.is_empty() {
            continue;
        }

        let destination = skill_root.join(Path::new(relative));
        let content = EmbeddedSkills::get(asset_path).ok_or_else(|| {
            TsqError::new(
                "INTERNAL_ERROR",
                format!("embedded skill asset missing: {}", asset_path),
                2,
            )
        })?;

        write_embedded_file(&destination, content.data.as_ref())?;

        if asset_path == format!("{}/SKILL.md", skill_name) {
            found = true;
        }
    }

    if !found {
        return Err(TsqError::new(
            "VALIDATION_ERROR",
            format!(
                "embedded skill source not found for '{}' (expected SKILLS/{}/SKILL.md)",
                skill_name, skill_name
            ),
            1,
        ));
    }

    Ok(EmbeddedSkillMaterialization {
        skill_name: skill_name.to_string(),
        skill_root,
        temp_root,
    })
}

fn write_embedded_file(destination: &Path, contents: &[u8]) -> Result<(), TsqError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            TsqError::new("IO_ERROR", "failed creating embedded skill directory", 2)
                .with_details(io_error_value(&e))
        })?;
    }

    fs::write(destination, contents).map_err(|e| {
        TsqError::new("IO_ERROR", "failed writing embedded skill file", 2)
            .with_details(io_error_value(&e))
    })?;

    Ok(())
}

fn io_error_value(error: &std::io::Error) -> serde_json::Value {
    serde_json::json!({
        "kind": format!("{:?}", error.kind()),
        "message": error.to_string()
    })
}
