use serde::Serialize;
use sqlx::SqlitePool;

pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
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
