use crate::authorization::{PermissionPolicy, PermissionRequirement, PolicyMode};
use crate::types::AppState;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/admin/workshop", get(workshop_page))
        .with_state(state)
}

async fn workshop_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session_token) = session_token(&headers) else {
        return plain_response(
            StatusCode::UNAUTHORIZED,
            "Sign in to view the Workshop registry.",
        );
    };
    let Some(authorizer) = state.authorization.as_ref() else {
        return plain_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "Registry authorization is temporarily unavailable.",
        );
    };
    let policy = PermissionPolicy {
        mode: PolicyMode::All,
        requirements: vec![PermissionRequirement::Role {
            role: "root".to_owned(),
        }],
    };
    match authorizer.allows(&session_token, &policy).await {
        Ok(true) => {}
        Ok(false) => {
            return plain_response(StatusCode::FORBIDDEN, "Root access is required.");
        }
        Err(()) => {
            return plain_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Registry authorization is temporarily unavailable.",
            );
        }
    }

    let entries = match crate::db::admin_catalog(&state.pool).await {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("registry admin catalog query failed: {error}");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The catalog could not be loaded.",
            );
        }
    };
    html_response(StatusCode::OK, render_catalog(&entries))
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| {
            let valid = name == "vapor_identity_session"
                && (32..=256).contains(&value.len())
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
            valid.then(|| value.to_owned())
        })
}

fn render_catalog(entries: &[crate::types::AdminCatalogEntry]) -> String {
    let rows = entries
        .iter()
        .map(|entry| {
            let source = match (&entry.provider, &entry.owner, &entry.repository) {
                (Some(provider), Some(owner), Some(repository)) => {
                    format!("{provider}:{owner}/{repository}")
                }
                _ => "No source repository recorded".to_owned(),
            };
            let source_kind = entry
                .source_kind
                .as_deref()
                .map(escape_html)
                .unwrap_or_else(|| "—".to_owned());
            let checkout = entry
                .checkout_path
                .as_deref()
                .map(escape_html)
                .unwrap_or_else(|| "—".to_owned());
            format!(
                "<tr><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>None recorded</td><td>Not tracked</td></tr>",
                escape_html(&entry.display_name),
                escape_html(&entry.ecosystem_id),
                escape_html(&entry.visibility),
                escape_html(&source),
                source_kind,
                checkout,
            )
        })
        .collect::<String>();
    let rows = if entries.is_empty() {
        "<tr><td colspan=\"8\">No catalog entries are recorded.</td></tr>".to_owned()
    } else {
        rows
    };
    let count = entries.len();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Workshop Registry · Vapor Admin</title><link rel=\"stylesheet\" href=\"/admin/assets/admin.css\"><script defer src=\"/admin/assets/admin.js\"></script></head><body><main id=\"admin-content\"><p class=\"eyebrow\">Workshop · Registry</p><h1>Catalog</h1><p class=\"muted\">Read-only registry inventory · {count} rows shown (maximum 200)</p><div class=\"card\"><table><thead><tr><th>Ecosystem</th><th>Canonical ID</th><th>Visibility</th><th>Source</th><th>Source kind</th><th>Checkout path</th><th>Workshop binding</th><th>Release channel</th></tr></thead><tbody>{rows}</tbody></table><p class=\"muted\">Publishing and catalog mutation are not available in this view.</p></div></main></body></html>"
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn plain_response(status: StatusCode, message: &'static str) -> Response {
    let body = if status == StatusCode::UNAUTHORIZED {
        format!("{message} <a href=\"/login\">Sign in</a>")
    } else {
        message.to_owned()
    };
    response(status, "text/html; charset=utf-8", body)
}

fn html_response(status: StatusCode, body: String) -> Response {
    response(status, "text/html; charset=utf-8", body)
}

fn response(status: StatusCode, content_type: &'static str, body: String) -> Response {
    let mut response = (
        status,
        [(CONTENT_TYPE, HeaderValue::from_static(content_type))],
        body,
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

#[cfg(test)]
mod tests {
    use crate::authorization::IdentityAuthorizationClient;
    use crate::db;
    use crate::types::AppState;
    use axum::body::to_bytes;
    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::net::TcpListener;

    const SESSION: &str =
        "vapor-session-12345678-1234-4234-9234-123456789abc-12345678-1234-4234-9234-123456789abc";
    const SERVICE_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    async fn identity_check(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
        let authorized = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == format!("Bearer {SERVICE_TOKEN}"))
            && body["audience"] == "vapor-registry"
            && body["session_token"] == SESSION
            && body["policy"]["requirements"][0]["role"] == "root";
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 60;
        Json(json!({"allowed": authorized, "expires_at_unix": expires_at}))
    }

    async fn test_state() -> (AppState, tokio::task::JoinHandle<()>) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::initialize(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO ecosystems(id, namespace, name, display_name) VALUES (?, ?, ?, ?)",
        )
        .bind("test/xss")
        .bind("test")
        .bind("xss")
        .bind("<img src=x onerror=alert(1)>")
        .execute(&pool)
        .await
        .unwrap();

        let identity = Router::new().route("/authorize", post(identity_check));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/authorize", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, identity).await.unwrap();
        });
        let client = reqwest::Client::new();
        let authorization = IdentityAuthorizationClient::for_test(
            reqwest::Url::parse(&endpoint).unwrap(),
            SERVICE_TOKEN.to_owned(),
            client,
        );
        (
            AppState {
                pool,
                authorization: Some(authorization),
            },
            server,
        )
    }

    fn session_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_str(&format!("vapor_identity_session={SESSION}")).unwrap(),
        );
        headers
    }

    #[tokio::test]
    async fn workshop_admin_requires_root_session() {
        let (state, server) = test_state().await;
        let response = super::workshop_page(State(state), HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "private, no-store"
        );
        server.abort();
    }

    #[tokio::test]
    async fn workshop_admin_lists_bounded_catalog_and_escapes_labels() {
        let (state, server) = test_state().await;
        let response = super::workshop_page(State(state), session_headers()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("Vapor"));
        assert!(html.contains("ghf-studios/vapor"));
        assert!(html.contains("Not tracked"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(!html.contains("<img src=x onerror=alert(1)>"));
        assert!(html.contains("Publishing and catalog mutation are not available"));
        assert!(!html.contains("<button"));
        assert!(!html.contains("<form"));
        server.abort();
    }

    #[test]
    fn html_escaping_covers_text_and_attribute_delimiters() {
        assert_eq!(super::escape_html("<&\"'>"), "&lt;&amp;&quot;&#39;&gt;");
    }
}
