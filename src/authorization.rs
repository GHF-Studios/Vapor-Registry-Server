use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::env;

const MAX_POLICY_REQUIREMENTS: usize = 16;

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
            .timeout(std::time::Duration::from_secs(3))
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
        if !response.status().is_success() {
            return Err(());
        }
        let decision = response
            .json::<AuthorizationResponse>()
            .await
            .map_err(|_| ())?;
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

fn is_service_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_loopback_url(url: &Url) -> bool {
    if url.scheme() != "http" || url.username() != "" || url.password().is_some() {
        return false;
    }
    matches!(
        url.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1")
    )
}
