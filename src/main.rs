use tasque::app::TasqueService;
use tasque::app::runtime::{get_actor, get_repo_root, now_iso};
use tasque::app::sync;
use tasque::cli::action::{GlobalOpts, emit_error};
use tasque::cli::preparse::preparse_args;
use tasque::cli::run_cli;

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    let preparse = preparse_args(&raw_args);
    let repo_root = if let Some(root) = preparse.root.clone() {
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(root)
        };
        if let Err(error) = std::env::set_current_dir(&root) {
            let exit_code = emit_error(
                "tsq",
                GlobalOpts {
                    json: preparse.wants_json(),
                    plain: false,
                    exact_id: false,
                    explicit_root: true,
                },
                tasque::TsqError::new("IO_ERROR", "failed applying --root", 2)
                    .with_details(serde_json::json!({ "error": error.to_string() })),
            );
            std::process::exit(exit_code);
        }
        root
    } else if should_initialize_cwd(&preparse) {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        get_repo_root()
    };
    let actor = get_actor(&repo_root);

    let effective_root = if should_use_repo_root(&preparse) || preparse.display_only {
        repo_root.to_string_lossy().to_string()
    } else {
        match sync::resolve_effective_root(&repo_root.to_string_lossy()) {
            Ok(root) => root,
            Err(error) => {
                let exit_code = emit_error(
                    "tsq",
                    GlobalOpts {
                        json: preparse.wants_json(),
                        plain: false,
                        exact_id: false,
                        explicit_root: false,
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

fn should_initialize_cwd(preparse: &tasque::cli::preparse::PreparseResult) -> bool {
    matches!(preparse.command.as_deref(), Some("init"))
}

fn should_use_repo_root(preparse: &tasque::cli::preparse::PreparseResult) -> bool {
    let Some(command) = preparse.command.as_deref() else {
        return false;
    };
    matches!(
        command,
        "init" | "doctor" | "root" | "migrate" | "merge-driver" | "skills"
    )
}
