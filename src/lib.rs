mod admin;
mod authorization;
mod db;
mod types;

use axum::extract::{Path as AxumPath, State};
use axum::http::header::COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use std::env;
use std::path::PathBuf;
use tokio::fs;

use crate::authorization::{IdentityAuthorizationClient, LocalSourceRequirement, PermissionPolicy};
use crate::types::{AppState, ErrorResponse, ProviderAccountResponse, RegistryStatus};

const DEFAULT_STATE_DIR: &str = "/var/lib/vapor-server/registry";

const DEFAULT_DB_NAME: &str = "registry.sqlite3";

pub struct RegistryRouters {
    pub api: Router,
    pub admin: Router,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn app_router_from_env() -> Result<Router, Box<dyn std::error::Error>> {
    let routers = routers_from_env().await?;
    Ok(routers.api.merge(routers.admin))
}

pub async fn routers_from_env() -> Result<RegistryRouters, Box<dyn std::error::Error>> {
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

    let state = AppState {
        pool,
        authorization: IdentityAuthorizationClient::from_env(),
    };

    let api = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/status", get(registry_status))
        .route("/v1/ecosystems/{namespace}/{name}", get(get_ecosystem))
        .route(
            "/v1/providers/{provider}/{login}",
            get(get_provider_account),
        )
        .with_state(state.clone());
    let admin = admin::router(state);
    Ok(RegistryRouters { api, admin })
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
    headers: HeaderMap,
) -> Response {
    let response = match db::ecosystem(&state.pool, &namespace, &name).await {
        Ok(response) => response,
        Err(error) => return internal_error(error).into_response(),
    };
    match response {
        Some(mut ecosystem) => {
            let policy =
                match db::ecosystem_access_policy(&state.pool, &ecosystem.ecosystem.id).await {
                    Ok(Some(policy)) => policy,
                    Ok(None) => return hidden_resource().into_response(),
                    Err(error) => return internal_error(error).into_response(),
                };
            let (schema_version, visibility, policy_json, local_json) = policy;
            if schema_version != 1 {
                return hidden_resource().into_response();
            }
            let local_requirements =
                match serde_json::from_str::<Vec<LocalSourceRequirement>>(&local_json) {
                    Ok(requirements) => requirements,
                    Err(_) => return hidden_resource().into_response(),
                };
            if visibility == "protected" {
                let Some(session_token) = session_token_from_cookie(&headers) else {
                    return hidden_resource().into_response();
                };
                let Ok(policy) = serde_json::from_str::<PermissionPolicy>(&policy_json) else {
                    return hidden_resource().into_response();
                };
                let Some(authorizer) = state.authorization.as_ref() else {
                    return hidden_resource().into_response();
                };
                match authorizer.allows(&session_token, &policy).await {
                    Ok(true) => {}
                    Ok(false) | Err(()) => {
                        return hidden_resource().into_response();
                    }
                }
            } else if visibility != "public" {
                return hidden_resource().into_response();
            }
            ecosystem.local_requirements = local_requirements;
            (
                StatusCode::OK,
                [("cache-control", "private, no-store")],
                Json(ecosystem),
            )
                .into_response()
        }

        None => hidden_resource().into_response(),
    }
}

fn session_token_from_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| {
            if name == "vapor_identity_session" && !value.is_empty() {
                Some(value.to_owned())
            } else {
                None
            }
        })
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

fn hidden_resource() -> Response {
    (
        StatusCode::NOT_FOUND,
        [("cache-control", "private, no-store")],
        Json(ErrorResponse {
            error: "resource not found".to_owned(),
        }),
    )
        .into_response()
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
