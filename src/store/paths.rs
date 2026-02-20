use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TasquePaths {
    pub tasque_dir: PathBuf,
    pub events_file: PathBuf,
    pub config_file: PathBuf,
    pub state_file: PathBuf,
    pub lock_file: PathBuf,
    pub snapshots_dir: PathBuf,
    pub specs_dir: PathBuf,
}

pub fn get_paths(repo_root: impl AsRef<Path>) -> TasquePaths {
    let tasque_dir = repo_root.as_ref().join(".tasque");
    TasquePaths {
        events_file: tasque_dir.join("events.jsonl"),
        config_file: tasque_dir.join("config.json"),
        state_file: tasque_dir.join("state.json"),
        lock_file: tasque_dir.join(".lock"),
        snapshots_dir: tasque_dir.join("snapshots"),
        specs_dir: tasque_dir.join("specs"),
        tasque_dir,
    }
}

pub fn task_spec_relative_path(task_id: &str) -> String {
    format!(".tasque/specs/{}/spec.md", task_id)
}

pub fn task_spec_file(repo_root: impl AsRef<Path>, task_id: &str) -> PathBuf {
    repo_root
        .as_ref()
        .join(".tasque")
        .join("specs")
        .join(task_id)
        .join("spec.md")
}
