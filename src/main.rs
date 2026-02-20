use tasque::app::TasqueService;
use tasque::app::runtime::{get_actor, get_repo_root, now_iso};
use tasque::cli::run_cli;

fn main() {
    let repo_root = get_repo_root();
    let actor = get_actor(&repo_root);
    let service = TasqueService::new(repo_root.to_string_lossy().to_string(), actor, now_iso);
    let exit_code = run_cli(&service);
    std::process::exit(exit_code);
}
