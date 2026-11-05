use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::env;

const MAX_POLICY_REQUIREMENTS: usize = 16;
const MAX_AUTH_RESPONSE_BYTES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PermissionPolicy {
    pub(crate) mode: PolicyMode,
    pub(crate) requirements: Vec<PermissionRequirement>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyMode {
    All,
    Any,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PermissionRequirement {
    ProductEntitlement {
        product_id: String,
        license_mode: LicenseMode,
    },
    Role {
        role: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LicenseMode {
    Active,
    Permanent,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalSourceRequirement {
    pub(crate) source_id: String,
    pub(crate) version: Option<String>,
}

#[derive(Serialize)]
struct AuthorizationRequest<'a> {
    audience: &'static str,
    session_token: &'a str,
    policy: &'a PermissionPolicy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationResponse {
    allowed: bool,
    expires_at_unix: i64,
}

#[derive(Clone)]
pub(crate) struct IdentityAuthorizationClient {
    url: Url,
    service_token: String,
    http: reqwest::Client,
}

impl IdentityAuthorizationClient {
    pub(crate) fn from_env() -> Option<Self> {
        let url = env::var("VAPOR_REGISTRY_IDENTITY_AUTHZ_URL").ok()?;
        let service_token = env::var("VAPOR_REGISTRY_IDENTITY_AUTHZ_TOKEN")
            .ok()
            .filter(|value| is_service_token(value))?;
        let url = Url::parse(&url).ok()?;
        if !is_loopback_url(&url) {
            return None;
        }
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(1))
            .timeout(std::time::Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Vapor-Registry-Server/0.1")
            .build()
            .ok()?;
        Some(Self {
            url,
            service_token,
            http,
        })
    }

    pub(crate) async fn allows(
        &self,
        session_token: &str,
        policy: &PermissionPolicy,
    ) -> Result<bool, ()> {
        if policy.requirements.is_empty() || policy.requirements.len() > MAX_POLICY_REQUIREMENTS {
            return Err(());
        }
        let response = self
            .http
            .post(self.url.clone())
            .bearer_auth(&self.service_token)
            .json(&AuthorizationRequest {
                audience: "vapor-registry",
                session_token,
                policy,
            })
            .send()
            .await
            .map_err(|_| ())?;
        if response.status() != reqwest::StatusCode::OK
            || response
                .content_length()
                .is_some_and(|length| length > MAX_AUTH_RESPONSE_BYTES as u64)
        {
            return Err(());
        }
        let body = read_bounded_response(response).await?;
        let decision: AuthorizationResponse = serde_json::from_slice(&body).map_err(|_| ())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| ())?
            .as_secs() as i64;
        Ok(decision.allowed && decision.expires_at_unix > now)
    }

    #[cfg(test)]
    pub(crate) fn for_test(url: Url, service_token: String, http: reqwest::Client) -> Self {
        Self {
            url,
            service_token,
            http,
        }
    }
}

async fn read_bounded_response(mut response: reqwest::Response) -> Result<Vec<u8>, ()> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if body.len().saturating_add(chunk.len()) > MAX_AUTH_RESPONSE_BYTES {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn is_service_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_loopback_url(url: &Url) -> bool {
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    matches!(
        url.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1")
    )
}

#[cfg(test)]
mod tests {
    use super::{IdentityAuthorizationClient, PermissionPolicy, PermissionRequirement, PolicyMode};
    use axum::routing::post;
    use axum::Router;
    use reqwest::Url;
    use std::time::Duration;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn rejects_identity_authorization_response_over_the_size_limit() {
        let body = format!(
            "{{\"allowed\":true,\"expires_at_unix\":{}}}{}",
            i64::MAX,
            " ".repeat(5000)
        );
        let app = Router::new().route(
            "/authorize",
            post(move || {
                let body = body.clone();
                async move { body }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let client = IdentityAuthorizationClient::for_test(
            Url::parse(&format!("http://{address}/authorize")).unwrap(),
            "A".repeat(64),
            http,
        );
        let policy = PermissionPolicy {
            mode: PolicyMode::All,
            requirements: vec![PermissionRequirement::Role {
                role: "root".to_owned(),
            }],
        };

        assert!(client.allows("session-token", &policy).await.is_err());
        server.abort();
    }
}
