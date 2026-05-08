mod common;

use common::{create_task, init_repo};
use tasque::app::service::TasqueService;
use tasque::app::service_types::DepTreeInput;
use tasque::cli::action::GlobalOpts;
use tasque::cli::commands::dep::{
    BlockArgs, DepsArgs, OrderArgs, UnblockArgs, UnorderArgs, execute_block, execute_deps,
    execute_order, execute_unblock, execute_unorder,
};
use tasque::cli::commands::label::{
    LabelArgs, UnlabelArgs, execute_label_add, execute_labels, execute_unlabel,
};
use tasque::cli::commands::link::{RelateArgs, UnrelateArgs, execute_relate, execute_unrelate};
use tasque::types::DependencyType;

#[test]
fn block_and_unblock_mutate_blocking_dependency() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let child = create_task(repo.path(), "Child");
    let blocker = create_task(repo.path(), "Blocker");
    let service = service_for(repo.path());
    let opts = opts();

    let code = execute_block(
        &service,
        BlockArgs {
            child: child.clone(),
            by: "by".to_string(),
            blocker: blocker.clone(),
        },
        opts,
    );

    assert_eq!(code, 0);
    let tree = service
        .dep_tree(DepTreeInput {
            id: child.clone(),
            direction: None,
            depth: None,
            exact_id: false,
        })
        .expect("dep tree");
    assert_eq!(tree.children[0].id, blocker);
    assert_eq!(tree.children[0].dep_type, Some(DependencyType::Blocks));

    let code = execute_unblock(
        &service,
        UnblockArgs {
            child: child.clone(),
            by: "by".to_string(),
            blocker,
        },
        opts,
    );

    assert_eq!(code, 0);
    let tree = service
        .dep_tree(DepTreeInput {
            id: child,
            direction: None,
            depth: None,
            exact_id: false,
        })
        .expect("dep tree after unblock");
    assert!(tree.children.is_empty());
}

#[test]
fn order_unorder_and_deps_use_starts_after_dependency() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let later = create_task(repo.path(), "Later");
    let earlier = create_task(repo.path(), "Earlier");
    let service = service_for(repo.path());
    let opts = opts();

    let code = execute_order(
        &service,
        OrderArgs {
            later: later.clone(),
            after: "after".to_string(),
            earlier: earlier.clone(),
        },
        opts,
    );

    assert_eq!(code, 0);
    assert_eq!(
        service
            .show(&later, false)
            .expect("show later")
            .blocker_edges[0]
            .dep_type,
        DependencyType::StartsAfter
    );
    assert_eq!(
        execute_deps(
            &service,
            DepsArgs {
                id: later.clone(),
                direction: "up".to_string(),
                depth: Some("1".to_string()),
            },
            opts,
        ),
        0
    );

    let code = execute_unorder(
        &service,
        UnorderArgs {
            later: later.clone(),
            after: "after".to_string(),
            earlier,
        },
        opts,
    );

    assert_eq!(code, 0);
    assert!(
        service
            .show(&later, false)
            .expect("show later after unorder")
            .blocker_edges
            .is_empty()
    );
}

#[test]
fn relate_and_unrelate_mutate_bidirectional_relation() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let a = create_task(repo.path(), "A");
    let b = create_task(repo.path(), "B");
    let service = service_for(repo.path());
    let opts = opts();

    assert_eq!(
        execute_relate(
            &service,
            RelateArgs {
                a: a.clone(),
                b: b.clone(),
            },
            opts,
        ),
        0
    );
    assert!(service.show(&a, false).expect("show a").links["relates_to"].contains(&b));
    assert!(service.show(&b, false).expect("show b").links["relates_to"].contains(&a));

    assert_eq!(
        execute_unrelate(
            &service,
            UnrelateArgs {
                a: a.clone(),
                b: b.clone(),
            },
            opts,
        ),
        0
    );
    assert!(
        !service
            .show(&a, false)
            .expect("show a after unrelate")
            .links
            .get("relates_to")
            .is_some_and(|links| links.contains(&b))
    );
}

#[test]
fn label_unlabel_and_labels_use_existing_label_service() {
    let repo = common::make_repo();
    init_repo(repo.path());
    let id = create_task(repo.path(), "Label target");
    let service = service_for(repo.path());
    let opts = opts();

    assert_eq!(
        execute_label_add(
            &service,
            LabelArgs {
                id: id.clone(),
                label: "design".to_string(),
            },
            opts,
        ),
        0
    );
    assert_eq!(execute_labels(&service, opts), 0);
    assert_eq!(service.label_list().expect("labels")[0].label, "design");

    assert_eq!(
        execute_unlabel(
            &service,
            UnlabelArgs {
                id,
                label: "design".to_string(),
            },
            opts,
        ),
        0
    );
    assert!(
        service
            .label_list()
            .expect("labels after unlabel")
            .is_empty()
    );
}

#[test]
fn malformed_sentence_tokens_return_validation_error_with_example() {
    let repo = common::make_repo();
    let service = service_for(repo.path());

    let code = execute_block(
        &service,
        BlockArgs {
            child: "tsq-aaaaaaaa".to_string(),
            by: "from".to_string(),
            blocker: "tsq-bbbbbbbb".to_string(),
        },
        opts(),
    );

    assert_eq!(code, 1);
    let error = tasque::cli::commands::dep::validate_sentence_token(
        "from",
        "by",
        "tsq block <task> by <blocker>",
    )
    .expect_err("invalid token should fail");
    assert_eq!(error.code, "VALIDATION_ERROR");
    assert!(error.message.contains("tsq block <task> by <blocker>"));
}

fn service_for(repo: &std::path::Path) -> TasqueService {
    TasqueService::new(repo.display().to_string(), "rust-test", || {
        "2026-05-08T00:00:00Z".to_string()
    })
}

fn opts() -> GlobalOpts {
    GlobalOpts {
        json: true,
        exact_id: false,
    }
}
