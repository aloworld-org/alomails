//! Real-database proof for the Sales → Projects conversion seam.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;
use alo_store::{
    AccountStore, CrmDealId, CrmPipelineId, CrmStageId, NewDeal, PipelineSeed, StageMove,
    StageSeed, Store, StoreError,
};

async fn account(store: &Store, tag: &str) -> AccountStore {
    let tenant = store
        .create_tenant(&format!("deal-project-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@deal-project.test"))
        .await
        .unwrap();
    store.for_account(tenant, user)
}

async fn board(store: &AccountStore) -> (CrmPipelineId, Vec<CrmStageId>) {
    let pipeline = store
        .crm_pipelines_or_seed(&PipelineSeed {
            name: "Sales".to_owned(),
            stages: vec![
                StageSeed {
                    name: "Open".to_owned(),
                    is_won: false,
                    is_lost: false,
                },
                StageSeed {
                    name: "Won".to_owned(),
                    is_won: true,
                    is_lost: false,
                },
                StageSeed {
                    name: "Lost".to_owned(),
                    is_won: false,
                    is_lost: true,
                },
            ],
        })
        .await
        .unwrap()
        .remove(0);
    let stages = store.crm_stages(&pipeline.id, false).await.unwrap();
    (
        pipeline.id,
        stages.into_iter().map(|stage| stage.id).collect(),
    )
}

async fn deal(store: &AccountStore, won: bool) -> CrmDealId {
    let (pipeline, stages) = board(store).await;
    let deal = store
        .create_crm_deal(
            &pipeline,
            &stages[0],
            &NewDeal {
                title: "Premium rollout".to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    if won {
        store
            .move_crm_deal(&deal, &StageMove::to(stages[1].clone()))
            .await
            .unwrap();
    }
    deal
}

#[tokio::test]
async fn a_won_deal_creates_one_bidirectionally_linked_project_even_on_retry() {
    let store = common::test_store().await;
    let account = account(&store, "happy").await;
    let deal = deal(&account, true).await;

    let (first, created) = account
        .create_project_from_won_deal(&deal, "Premium rollout", Some("#E76F51"), None)
        .await
        .unwrap();
    assert!(created);
    assert_eq!(first.project_name, "Premium rollout");
    assert_eq!(
        account.crm_deal_project(&deal).await.unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        account.crm_project_deal(&first.project_id).await.unwrap(),
        Some(first.clone())
    );

    let (retry, created) = account
        .create_project_from_won_deal(&deal, "Ignored retry name", None, None)
        .await
        .unwrap();
    assert!(!created);
    assert_eq!(retry, first);
    assert!(matches!(
        account.delete_crm_deal(&deal).await,
        Err(StoreError::Conflict(message)) if message.contains("delivery project")
    ));
    assert_eq!(
        account
            .task_projects()
            .await
            .unwrap()
            .iter()
            .filter(|p| p.kind == "team")
            .count(),
        1
    );
}

#[tokio::test]
async fn an_open_deal_is_refused_without_leaving_a_project() {
    let store = common::test_store().await;
    let account = account(&store, "open").await;
    let deal = deal(&account, false).await;
    let result = account
        .create_project_from_won_deal(&deal, "Too early", None, None)
        .await;
    assert!(matches!(result, Err(StoreError::Validation(message)) if message.contains("won")));
    assert!(account.crm_deal_project(&deal).await.unwrap().is_none());
    assert_eq!(
        account
            .task_projects()
            .await
            .unwrap()
            .iter()
            .filter(|p| p.kind == "team")
            .count(),
        0
    );
}

#[tokio::test]
async fn another_tenant_cannot_read_or_convert_the_relationship() {
    let store = common::test_store().await;
    let owner = account(&store, "owner").await;
    let neighbour = account(&store, "neighbour").await;
    let deal = deal(&owner, true).await;
    let (relationship, _) = owner
        .create_project_from_won_deal(&deal, "Private engagement", None, None)
        .await
        .unwrap();

    assert!(neighbour.crm_deal_project(&deal).await.unwrap().is_none());
    assert!(
        neighbour
            .crm_project_deal(&relationship.project_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        neighbour
            .create_project_from_won_deal(&deal, "Intrusion", None, None)
            .await,
        Err(StoreError::NotFound)
    ));
}
