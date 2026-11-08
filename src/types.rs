use serde::Serialize;
use sqlx::SqlitePool;

use crate::authorization::LocalSourceRequirement;

pub const REGISTRY_SCHEMA_VERSION: u32 = 3;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub authorization: Option<crate::authorization::IdentityAuthorizationClient>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct RegistryStatus {
    pub schema_version: u32,
    pub service: &'static str,
    pub ecosystems: i64,
    pub provider_accounts: i64,
    pub repositories: i64,
}

#[derive(Debug)]
pub struct AdminEcosystemRow {
    pub ecosystem_id: String,
    pub display_name: String,
    pub visibility: String,
    pub provider: Option<String>,
    pub owner: Option<String>,
    pub repository: Option<String>,
    pub source_kind: Option<String>,
    pub checkout_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Ecosystem {
    pub id: String,
    pub namespace: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct EcosystemRepository {
    pub id: String,
    pub provider: String,
    pub owner: String,
    pub name: String,
    pub kind: String,
    pub checkout_path: String,
    pub web_url: String,
    pub clone_url: String,
}

#[derive(Debug, Serialize)]
pub struct EcosystemResponse {
    pub schema_version: u32,
    pub ecosystem: Ecosystem,
    pub repositories: Vec<EcosystemRepository>,
    pub local_requirements: Vec<LocalSourceRequirement>,
}

#[derive(Debug, Serialize)]
pub struct ProviderAccount {
    pub id: String,
    pub provider: String,
    pub kind: String,
    pub login: String,
    pub canonical_url: String,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderRepository {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub web_url: String,
    pub clone_url: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderAccountResponse {
    pub schema_version: u32,
    pub account: ProviderAccount,
    pub repositories: Vec<ProviderRepository>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Project {
    pub id: String,
    pub namespace: String,
    pub slug: String,
    pub display_name: String,
    pub summary: String,
    pub state: String,
}
#[derive(Serialize)]
pub(crate) struct AccessibleProject {
    #[serde(flatten)]
    pub project: Project,
    pub role: String,
}
