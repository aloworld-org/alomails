//! Real-database proof that optional project setup is retry-safe and isolated.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{AccountStore, ProjectSetupPlan, Store, StoreError};

use crate::common;

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("project-setup-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@project-setup.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

#[tokio::test]
async fn a_retry_adds_no_second_space_room_or_task() {
    let store = common::test_store().await;
    let account = account(&store, "retry").await;
    let project = account
        .create_task_project("Premium rollout", None)
        .await
        .unwrap();
    let plan = ProjectSetupPlan {
        create_files_space: true,
        create_chat_room: true,
        kickoff: None,
        starter_tasks: vec!["Confirm scope".to_owned(), "Plan kickoff".to_owned()],
    };

    let first = account.setup_project(&project, &plan).await.unwrap();
    let retry = account.setup_project(&project, &plan).await.unwrap();
    assert_eq!(retry.space_id, first.space_id);
    assert_eq!(retry.chat_channel_id, first.chat_channel_id);
    assert_eq!(retry.starter_task_ids, first.starter_task_ids);
    assert_eq!(account.spaces().await.unwrap().len(), 1);
    assert_eq!(account.channels().await.unwrap().len(), 1);
    assert_eq!(account.tasks_in_project(&project).await.unwrap().len(), 2);
}

#[tokio::test]
async fn another_tenants_project_is_not_a_setup_target_or_readable_record() {
    let store = common::test_store().await;
    let owner = account(&store, "owner").await;
    let neighbour = account(&store, "neighbour").await;
    let project = owner
        .create_task_project("Private delivery", None)
        .await
        .unwrap();

    assert!(matches!(
        neighbour
            .setup_project(&project, &ProjectSetupPlan::default())
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        neighbour.project_setup(&project).await,
        Err(StoreError::NotFound)
    ));
    assert!(neighbour.spaces().await.unwrap().is_empty());
    assert!(neighbour.channels().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_empty_setup_plan_does_not_create_a_setup_record() {
    let store = common::test_store().await;
    let account = account(&store, "empty").await;
    let project = account
        .create_task_project("No resources yet", None)
        .await
        .unwrap();

    assert!(matches!(
        account
            .setup_project(&project, &ProjectSetupPlan::default())
            .await,
        Err(StoreError::Validation(_))
    ));
    assert_eq!(account.project_setup(&project).await.unwrap(), None);
}

#[tokio::test]
async fn concurrent_confirmations_create_each_resource_once() {
    let store = common::test_store().await;
    let account = account(&store, "concurrent").await;
    let project = account
        .create_task_project("Concurrent rollout", None)
        .await
        .unwrap();
    let plan = ProjectSetupPlan {
        create_files_space: true,
        create_chat_room: true,
        kickoff: None,
        starter_tasks: vec!["Confirm scope".to_owned()],
    };

    let (left, right) = tokio::join!(
        account.setup_project(&project, &plan),
        account.setup_project(&project, &plan),
    );
    assert_eq!(left.unwrap(), right.unwrap());
    assert_eq!(account.spaces().await.unwrap().len(), 1);
    assert_eq!(account.channels().await.unwrap().len(), 1);
    assert_eq!(account.tasks_in_project(&project).await.unwrap().len(), 1);
}
