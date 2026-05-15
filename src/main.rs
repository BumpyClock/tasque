use tasque::app::TasqueService;
use tasque::app::runtime::{get_actor, get_repo_root, now_iso};
use tasque::app::sync;
use tasque::cli::action::{GlobalOpts, emit_error};
use tasque::cli::run_cli;

fn main() {
    let repo_root = get_repo_root();
    let actor = get_actor(&repo_root);

    let effective_root = if should_use_repo_root() {
        repo_root.to_string_lossy().to_string()
    } else {
        match sync::resolve_effective_root(&repo_root.to_string_lossy()) {
            Ok(root) => root,
            Err(error) => {
                let wants_json = preparse_wants_json();
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
        }
    };

    let service = TasqueService::new(effective_root, actor, now_iso);
    let exit_code = run_cli(&service);
    std::process::exit(exit_code);
}

fn should_use_repo_root() -> bool {
    let Some(command) = preparse_command() else {
        return false;
    };
    matches!(
        command.as_str(),
        "init" | "migrate" | "merge-driver" | "skills"
    )
}

fn preparse_wants_json() -> bool {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--json" || arg == "--format=json" {
            return true;
        }
        if arg == "--format"
            && let Some(value) = args.next()
            && value == "json"
        {
            return true;
        }
    }
    false
}

fn preparse_command() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--format" {
            args.next();
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return Some(arg);
    }
    None
}
