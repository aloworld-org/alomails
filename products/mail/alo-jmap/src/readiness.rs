//! Process readiness: the exact build and schema serving this listener.

use alo_store::Store;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::state::AppState;

const BUILD_REVISION: &str = env!("ALO_BUILD_REVISION");
const BUILD_SCHEMA: i64 = parse_schema(env!("ALO_BUILD_SCHEMA"));

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ready {
    status: &'static str,
    revision: &'static str,
    build_schema: i64,
    database_schema: i64,
}

/// Reports ready only when the live database schema exactly matches this
/// binary. No tenant, account, or credential data crosses this endpoint.
pub async fn ready(State(state): State<AppState>) -> Response {
    response(&state.store).await
}

async fn response(store: &Store) -> Response {
    let Ok(database_schema) = store.migration_version().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let status = if database_schema == BUILD_SCHEMA {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(Ready {
            status: if status == StatusCode::OK {
                "ready"
            } else {
                "schema_mismatch"
            },
            revision: BUILD_REVISION,
            build_schema: BUILD_SCHEMA,
            database_schema,
        }),
    )
        .into_response()
}

const fn parse_schema(value: &str) -> i64 {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut result = 0_i64;
    while index < bytes.len() {
        let digit = bytes[index] - b'0';
        result = result * 10 + digit as i64;
        index += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::BlobStore;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn ready_names_the_build_and_matching_schema() -> Result<(), Box<dyn std::error::Error>> {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&alo_test_db::url())
            .await?;
        let store = Store::new(pool, BlobStore::in_memory(1024));
        store.migrate().await?;

        let response = response(&store).await;
        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
