use tasque::app::TasqueService;
use tasque::app::runtime::{get_actor, get_repo_root, now_iso};
use tasque::app::sync;
use tasque::cli::action::{GlobalOpts, emit_error};
use tasque::cli::run_cli;

fn main() {
    let repo_root = get_repo_root();
    let actor = get_actor(&repo_root);

    let effective_root = match sync::resolve_effective_root(&repo_root.to_string_lossy()) {
        Ok(root) => root,
        Err(error) => {
            let wants_json = std::env::args().any(|arg| arg == "--json");
            let exit_code = emit_error(
                "tsq",
                GlobalOpts {
                    json: wants_json,
                    exact_id: false,
                },
                error,
            );
            std::process::exit(exit_code);
        }
    };

    let service = TasqueService::new(effective_root, actor, now_iso);
    let exit_code = run_cli(&service);
    std::process::exit(exit_code);
}
