use super::*;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn project_migration_seeds_separate_directory_idempotently() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::initialize(&pool).await.unwrap();
    db::initialize(&pool).await.unwrap();
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT id,state FROM projects ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        rows,
        vec![
            ("ghf-studios/loo-cast".into(), "active".into()),
            ("ghf-studios/mad-max-modding".into(), "active".into())
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ecosystems")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    for (namespace, slug, state) in [
        ("bad/name", "ok", "active"),
        ("ok", "BAD", "active"),
        ("ok", "ok", "deleted"),
    ] {
        assert!(
            sqlx::query("INSERT INTO projects VALUES (?, ?, ?, 'Name', 'Summary', ?)")
                .bind(format!("{namespace}/{slug}"))
                .bind(namespace)
                .bind(slug)
                .bind(state)
                .execute(&pool)
                .await
                .is_err()
        );
    }
}

use axum::routing::post;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

async fn serve(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), task)
}

#[tokio::test]
async fn projects_require_live_user_membership_and_fail_closed() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::initialize(&pool).await.unwrap();
    let reply = Arc::new(Mutex::new((
        StatusCode::OK,
        json!({"memberships": [
        {"project_id":"ghf-studios/loo-cast","role":"developer"},
        {"project_id":"ghf-studios/mad-max-modding","role":"viewer"},
        {"project_id":"unknown/missing","role":"owner"}], "expires_at_unix": i64::MAX})
        .to_string(),
    )));
    let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let identity = Router::new().route(
        "/api/identity/v1/internal/projects/{operation}",
        post({
            let reply = reply.clone();
            let observed = observed.clone();
            move |headers: HeaderMap, Json(body): Json<Value>| {
                assert_eq!(
                    headers.get("authorization").unwrap(),
                    &format!("Bearer {}", "a".repeat(64))
                );
                observed.lock().unwrap().push(body);
                let (status, body) = reply.lock().unwrap().clone();
                async move { (status, body) }
            }
        }),
    );
    let (identity_url, identity_task) = serve(identity).await;
    let client = reqwest::Client::new();
    let state = AppState {
        pool: pool.clone(),
        authorization: Some(IdentityAuthorizationClient::for_test(
            format!("{identity_url}/api/identity/v1/internal/authorization/check")
                .parse()
                .unwrap(),
            "a".repeat(64),
            client.clone(),
        )),
    };
    let (base, task) = serve(project_router(state)).await;
    let cookie = format!("vapor_user_session={}", "s".repeat(64));
    let request = |path: &str| {
        client
            .get(format!("{base}{path}"))
            .header("cookie", &cookie)
    };
    for cookie_value in [
        "vapor_identity_session=root",
        "vapor_user_session=short",
        "",
        "vapor_user_session=bad; vapor_user_session=ambiguous",
    ] {
        let response = client
            .get(format!("{base}/v1/me/projects"))
            .header("cookie", cookie_value)
            .header("authorization", "Bearer root")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
    }
    assert!(observed.lock().unwrap().is_empty());
    sqlx::query("UPDATE projects SET state='archived' WHERE slug='mad-max-modding'")
        .execute(&pool)
        .await
        .unwrap();
    let html_label = "<img src=x onerror=alert(1)>";
    sqlx::query("UPDATE projects SET display_name = ? WHERE slug='loo-cast'")
        .bind(html_label)
        .execute(&pool)
        .await
        .unwrap();
    let response = request("/v1/me/projects").send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["projects"].as_array().unwrap().len(), 1);
    assert_eq!(body["projects"][0]["id"], "ghf-studios/loo-cast");
    assert_eq!(body["projects"][0]["role"], "developer");
    assert_eq!(body["projects"][0]["display_name"], html_label);
    assert_eq!(
        observed.lock().unwrap()[0],
        json!({"audience":"vapor-registry","session_token":"s".repeat(64)})
    );
    *reply.lock().unwrap() = (
        StatusCode::OK,
        json!({"allowed":true,"role":"viewer","expires_at_unix":i64::MAX}).to_string(),
    );
    let response = request("/v1/projects/ghf-studios/loo-cast")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["membership"]["role"], "viewer");
    assert_eq!(
        observed.lock().unwrap().last().unwrap(),
        &json!({"audience":"vapor-registry","session_token":"s".repeat(64),"project_id":"ghf-studios/loo-cast","minimum_role":"viewer"})
    );
    for path in [
        "/v1/projects/unknown/missing",
        "/v1/projects/ghf-studios/mad-max-modding",
    ] {
        assert_eq!(request(path).send().await.unwrap().status(), 404);
    }
    *reply.lock().unwrap() = (
        StatusCode::OK,
        json!({"allowed":false,"expires_at_unix":i64::MAX}).to_string(),
    );
    for path in [
        "/v1/projects/ghf-studios/loo-cast",
        "/v1/projects/ghf-studios/mad-max-modding",
        "/v1/projects/unknown/missing",
        "/v1/projects/INVALID/slug",
        "/v1/projects/ghf-studios/bad%2Fslug",
    ] {
        let response = request(path).send().await.unwrap();
        assert_eq!(response.status(), 404, "{path}");
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"error":"resource not found"})
        );
    }
    for (status, expected) in [
        (StatusCode::UNAUTHORIZED, 401),
        (StatusCode::FORBIDDEN, 503),
        (StatusCode::BAD_REQUEST, 503),
        (StatusCode::INTERNAL_SERVER_ERROR, 503),
    ] {
        *reply.lock().unwrap() = (status, String::new());
        for path in ["/v1/me/projects", "/v1/projects/ghf-studios/loo-cast"] {
            let response = request(path).send().await.unwrap();
            assert_eq!(response.status(), expected);
            assert_eq!(response.headers()["cache-control"], "private, no-store");
        }
    }
    for body in [json!({"memberships":[{"project_id":"ghf-studios/loo-cast","role":"root"}],"expires_at_unix":i64::MAX}).to_string(),json!({"memberships":[],"expires_at_unix":0}).to_string(),"x".repeat(70000),"{}".into(),json!({"memberships":[{"project_id":"bad","role":"viewer"}],"expires_at_unix":i64::MAX}).to_string()] {
        *reply.lock().unwrap()=(StatusCode::OK,body);
        assert_eq!(request("/v1/me/projects").send().await.unwrap().status(),503);
    }
    for body in [
        json!({"allowed":true,"role":"root","expires_at_unix":i64::MAX}),
        json!({"allowed":true,"expires_at_unix":i64::MAX}),
        json!({"allowed":true,"role":"owner","expires_at_unix":0}),
    ] {
        *reply.lock().unwrap() = (StatusCode::OK, body.to_string());
        assert_eq!(
            request("/v1/projects/ghf-studios/loo-cast")
                .send()
                .await
                .unwrap()
                .status(),
            503
        );
    }
    *reply.lock().unwrap() = (
        StatusCode::OK,
        json!({"memberships":[],"expires_at_unix":i64::MAX}).to_string(),
    );
    assert_eq!(
        request("/v1/me/projects")
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        json!({"projects":[]})
    );
    identity_task.abort();
    assert_eq!(
        request("/v1/me/projects").send().await.unwrap().status(),
        503
    );
    task.abort();
    let (base, task) = serve(project_router(AppState {
        pool,
        authorization: None,
    }))
    .await;
    assert_eq!(
        client
            .get(format!("{base}/v1/me/projects"))
            .header("cookie", cookie)
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    task.abort();
}

#[tokio::test]
async fn project_identity_transport_preserves_standalone_path_and_bounds_chunked_body() {
    use crate::authorization::ProjectAuthorizationError::Unavailable;
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(std::str::from_utf8(&bytes[..n])
            .unwrap()
            .starts_with("POST /v1/internal/projects/list HTTP/1.1"));
        // No Content-Length: the streaming bound must apply independently of headers.
        let chunk = "x".repeat(70000);
        let response = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n", chunk.len(), chunk);
        let _ = stream.write_all(response.as_bytes());
    });
    let client = IdentityAuthorizationClient::for_test(
        format!("http://{address}/v1/internal/authorization/check")
            .parse()
            .unwrap(),
        "a".repeat(64),
        reqwest::Client::new(),
    );
    assert!(matches!(
        client.list_projects(&"s".repeat(64)).await,
        Err(Unavailable)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn project_identity_configuration_rejects_unsafe_urls_and_tokens() {
    use crate::authorization::ProjectAuthorizationError::Unavailable;
    for url in [
        "https://127.0.0.1/v1/internal/authorization/check",
        "http://example.com/v1/internal/authorization/check",
        "http://user:password@127.0.0.1/v1/internal/authorization/check",
        "http://127.0.0.1/v1/internal/authorization/check?secret=query",
        "http://127.0.0.1/v1/internal/authorization/check#fragment",
        "http://127.0.0.1/not-an-authorization-route",
    ] {
        let client = IdentityAuthorizationClient::for_test(
            url.parse().unwrap(),
            "a".repeat(64),
            reqwest::Client::new(),
        );
        assert!(
            matches!(
                client.list_projects(&"s".repeat(64)).await,
                Err(Unavailable)
            ),
            "{url}"
        );
    }
    let client = IdentityAuthorizationClient::for_test(
        "http://127.0.0.1/v1/internal/authorization/check"
            .parse()
            .unwrap(),
        "invalid".into(),
        reqwest::Client::new(),
    );
    assert!(matches!(
        client.list_projects(&"s".repeat(64)).await,
        Err(Unavailable)
    ));
}
