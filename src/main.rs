mod db;
mod types;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use std::env;
use std::path::PathBuf;
use tokio::fs;
use tokio::net::TcpListener;

use crate::types::{
    AppState, EcosystemResponse, ErrorResponse, ProviderAccountResponse, RegistryStatus,
};

const DEFAULT_BIND: &str = "127.0.0.1:7115";

const DEFAULT_STATE_DIR: &str = "/var/lib/vapor-server/registry";

const DEFAULT_DB_NAME: &str = "registry.sqlite3";

type ApiError = (StatusCode, Json<ErrorResponse>);

type ApiResult<T> = Result<Json<T>, ApiError>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = env::var("VAPOR_REGISTRY_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());

    let state_dir = PathBuf::from(
        env::var("VAPOR_REGISTRY_STATE").unwrap_or_else(|_| DEFAULT_STATE_DIR.to_owned()),
    );

    fs::create_dir_all(&state_dir).await?;

    let db_path = env::var("VAPOR_REGISTRY_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| state_dir.join(DEFAULT_DB_NAME));

    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let pool = db::open_database(&db_path).await?;

    db::initialize(&pool).await?;

    let state = AppState { pool };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/status", get(registry_status))
        .route("/v1/ecosystems/{namespace}/{name}", get(get_ecosystem))
        .route(
            "/v1/providers/{provider}/{login}",
            get(get_provider_account),
        )
        .with_state(state);

    let listener = TcpListener::bind(&bind).await?;

    eprintln!("vapor-registry-server listening on {bind}");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn healthz() -> &'static str {
    "ok\n"
}

async fn registry_status(State(state): State<AppState>) -> ApiResult<RegistryStatus> {
    db::status(&state.pool)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn get_ecosystem(
    AxumPath((namespace, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<EcosystemResponse> {
    match db::ecosystem(&state.pool, &namespace, &name)
        .await
        .map_err(internal_error)?
    {
        Some(ecosystem) => Ok(Json(ecosystem)),

        None => Err(not_found(format!(
            "unknown Vapor ecosystem `{namespace}/{name}`"
        ))),
    }
}

async fn get_provider_account(
    AxumPath((provider, login)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<ProviderAccountResponse> {
    match db::provider_account(&state.pool, &provider, &login)
        .await
        .map_err(internal_error)?
    {
        Some(account) => Ok(Json(account)),

        None => Err(not_found(format!(
            "unknown Vapor provider account `{provider}:{login}`"
        ))),
    }
}

fn not_found(message: String) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: message }),
    )
}

fn internal_error(error: sqlx::Error) -> ApiError {
    eprintln!("registry database error: {error}");

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "registry database operation failed".to_owned(),
        }),
    )
}