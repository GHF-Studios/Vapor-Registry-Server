use crate::types::{
    Ecosystem, EcosystemRepository, EcosystemResponse, ProviderAccount, ProviderAccountResponse,
    ProviderRepository, RegistryStatus, REGISTRY_SCHEMA_VERSION,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::time::Duration;

const VAPOR_ECOSYSTEM_ID: &str = "ghf-studios/vapor";
const GHF_STUDIOS_ACCOUNT_ID: &str = "github:GHF-Studios";

pub async fn open_database(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

pub async fn initialize(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    run_migrations(pool).await?;
    seed_official_registry(pool).await?;

    Ok(())
}

async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    const MIGRATIONS: &[&str] = &[
        r#"
        CREATE TABLE IF NOT EXISTS registry_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS provider_accounts (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL COLLATE NOCASE,
            account_kind TEXT NOT NULL,
            login TEXT NOT NULL COLLATE NOCASE,
            canonical_url TEXT NOT NULL,
            verified INTEGER NOT NULL DEFAULT 0
                CHECK (verified IN (0, 1)),
            UNIQUE(provider, login)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS repositories (
            id TEXT PRIMARY KEY,
            provider_account_id TEXT NOT NULL
                REFERENCES provider_accounts(id),
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            web_url TEXT NOT NULL,
            clone_url TEXT NOT NULL,
            UNIQUE(provider_account_id, name)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS ecosystems (
            id TEXT PRIMARY KEY,
            namespace TEXT NOT NULL,
            name TEXT NOT NULL,
            display_name TEXT NOT NULL,
            UNIQUE(namespace, name)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS ecosystem_repositories (
            ecosystem_id TEXT NOT NULL
                REFERENCES ecosystems(id)
                ON DELETE CASCADE,
            repository_id TEXT NOT NULL
                REFERENCES repositories(id)
                ON DELETE CASCADE,
            checkout_path TEXT NOT NULL,
            ordinal INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(ecosystem_id, repository_id),
            UNIQUE(ecosystem_id, checkout_path)
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS repositories_provider_account_idx
            ON repositories(provider_account_id)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS ecosystem_repositories_ecosystem_idx
            ON ecosystem_repositories(ecosystem_id, ordinal)
        "#,
    ];

    for migration in MIGRATIONS {
        sqlx::query(migration).execute(pool).await?;
    }

    sqlx::query(
        r#"
        INSERT INTO registry_meta(key, value)
        VALUES ('schema_version', ?)
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value
        "#,
    )
    .bind(REGISTRY_SCHEMA_VERSION.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

/// Seed the official bootstrap records required to reconstruct the Vapor
/// ecosystem.
///
/// These are not Git history. They are Registry facts describing external
/// provider resources and the top-level source topology required for
/// acquisition.
async fn seed_official_registry(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO provider_accounts(
            id,
            provider,
            account_kind,
            login,
            canonical_url,
            verified
        )
        VALUES (?, 'github', 'organization', 'GHF-Studios',
                'https://github.com/GHF-Studios', 1)
        ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            account_kind = excluded.account_kind,
            login = excluded.login,
            canonical_url = excluded.canonical_url,
            verified = excluded.verified
        "#,
    )
    .bind(GHF_STUDIOS_ACCOUNT_ID)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO ecosystems(
            id,
            namespace,
            name,
            display_name
        )
        VALUES (?, 'ghf-studios', 'vapor', 'Vapor')
        ON CONFLICT(id) DO UPDATE SET
            namespace = excluded.namespace,
            name = excluded.name,
            display_name = excluded.display_name
        "#,
    )
    .bind(VAPOR_ECOSYSTEM_ID)
    .execute(&mut *transaction)
    .await?;

    let repositories = [
        (
            "github:GHF-Studios/Vapor-Root",
            "Vapor-Root",
            "container-repo",
            "https://github.com/GHF-Studios/Vapor-Root",
            "https://github.com/GHF-Studios/Vapor-Root.git",
        ),
        (
            "github:GHF-Studios/Vapor",
            "Vapor",
            "workspace",
            "https://github.com/GHF-Studios/Vapor",
            "https://github.com/GHF-Studios/Vapor.git",
        ),
        (
            "github:GHF-Studios/Vapor-Examples",
            "Vapor-Examples",
            "workspace",
            "https://github.com/GHF-Studios/Vapor-Examples",
            "https://github.com/GHF-Studios/Vapor-Examples.git",
        ),
        (
            "github:GHF-Studios/Vapor-Server-Root",
            "Vapor-Server-Root",
            "container-repo",
            "https://github.com/GHF-Studios/Vapor-Server-Root",
            "https://github.com/GHF-Studios/Vapor-Server-Root.git",
        ),
        (
            "github:GHF-Studios/Vapor-Registry-Server",
            "Vapor-Registry-Server",
            "workspace",
            "https://github.com/GHF-Studios/Vapor-Registry-Server",
            "https://github.com/GHF-Studios/Vapor-Registry-Server.git",
        ),
    ];

    for (id, name, kind, web_url, clone_url) in repositories {
        sqlx::query(
            r#"
            INSERT INTO repositories(
                id,
                provider_account_id,
                name,
                kind,
                web_url,
                clone_url
            )
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                provider_account_id = excluded.provider_account_id,
                name = excluded.name,
                kind = excluded.kind,
                web_url = excluded.web_url,
                clone_url = excluded.clone_url
            "#,
        )
        .bind(id)
        .bind(GHF_STUDIOS_ACCOUNT_ID)
        .bind(name)
        .bind(kind)
        .bind(web_url)
        .bind(clone_url)
        .execute(&mut *transaction)
        .await?;
    }

    // For ecosystem acquisition the Registry only needs to identify the
    // top-level Container Repos. Their authored `.gitmodules` remain
    // authoritative for contained Workspace revisions.
    sqlx::query(
        r#"
        DELETE FROM ecosystem_repositories
        WHERE ecosystem_id = ?
        "#,
    )
    .bind(VAPOR_ECOSYSTEM_ID)
    .execute(&mut *transaction)
    .await?;

    for (repository_id, checkout_path, ordinal) in [
        ("github:GHF-Studios/Vapor-Root", "Vapor-Root", 10_i64),
        (
            "github:GHF-Studios/Vapor-Server-Root",
            "Vapor-Server-Root",
            20_i64,
        ),
    ] {
        sqlx::query(
            r#"
            INSERT INTO ecosystem_repositories(
                ecosystem_id,
                repository_id,
                checkout_path,
                ordinal
            )
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(VAPOR_ECOSYSTEM_ID)
        .bind(repository_id)
        .bind(checkout_path)
        .bind(ordinal)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;

    Ok(())
}

pub async fn status(pool: &SqlitePool) -> Result<RegistryStatus, sqlx::Error> {
    let ecosystems = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ecosystems")
        .fetch_one(pool)
        .await?;

    let provider_accounts = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM provider_accounts")
        .fetch_one(pool)
        .await?;

    let repositories = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
        .fetch_one(pool)
        .await?;

    Ok(RegistryStatus {
        schema_version: REGISTRY_SCHEMA_VERSION,
        service: "vapor-registry-server",
        ecosystems,
        provider_accounts,
        repositories,
    })
}

pub async fn ecosystem(
    pool: &SqlitePool,
    namespace: &str,
    name: &str,
) -> Result<Option<EcosystemResponse>, sqlx::Error> {
    let Some((id, namespace, name, display_name)) =
        sqlx::query_as::<_, (String, String, String, String)>(
            r#"
            SELECT
                id,
                namespace,
                name,
                display_name
            FROM ecosystems
            WHERE namespace = ?
              AND name = ?
            "#,
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    >(
        r#"
            SELECT
                repositories.id,
                provider_accounts.provider,
                provider_accounts.login,
                repositories.name,
                repositories.kind,
                ecosystem_repositories.checkout_path,
                repositories.web_url,
                repositories.clone_url
            FROM ecosystem_repositories
            JOIN repositories
              ON repositories.id =
                 ecosystem_repositories.repository_id
            JOIN provider_accounts
              ON provider_accounts.id =
                 repositories.provider_account_id
            WHERE ecosystem_repositories.ecosystem_id = ?
            ORDER BY
                ecosystem_repositories.ordinal,
                repositories.id
            "#,
    )
    .bind(&id)
    .fetch_all(pool)
    .await?;

    let repositories = rows
        .into_iter()
        .map(
            |(id, provider, owner, name, kind, checkout_path, web_url, clone_url)| {
                EcosystemRepository {
                    id,
                    provider,
                    owner,
                    name,
                    kind,
                    checkout_path,
                    web_url,
                    clone_url,
                }
            },
        )
        .collect();

    Ok(Some(EcosystemResponse {
        schema_version: REGISTRY_SCHEMA_VERSION,

        ecosystem: Ecosystem {
            id,
            namespace,
            name,
            display_name,
        },

        repositories,
    }))
}

pub async fn provider_account(
    pool: &SqlitePool,
    provider: &str,
    login: &str,
) -> Result<Option<ProviderAccountResponse>, sqlx::Error> {
    let Some((id, provider, kind, login, canonical_url, verified)) =
        sqlx::query_as::<_, (String, String, String, String, String, i64)>(
            r#"
            SELECT
                id,
                provider,
                account_kind,
                login,
                canonical_url,
                verified
            FROM provider_accounts
            WHERE provider = ? COLLATE NOCASE
              AND login = ? COLLATE NOCASE
            "#,
        )
        .bind(provider)
        .bind(login)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        r#"
            SELECT
                id,
                name,
                kind,
                web_url,
                clone_url
            FROM repositories
            WHERE provider_account_id = ?
            ORDER BY kind, name
            "#,
    )
    .bind(&id)
    .fetch_all(pool)
    .await?;

    let repositories = rows
        .into_iter()
        .map(|(id, name, kind, web_url, clone_url)| ProviderRepository {
            id,
            name,
            kind,
            web_url,
            clone_url,
        })
        .collect();

    Ok(Some(ProviderAccountResponse {
        schema_version: REGISTRY_SCHEMA_VERSION,

        account: ProviderAccount {
            id,
            provider,
            kind,
            login,
            canonical_url,
            verified: verified != 0,
        },

        repositories,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn official_seed_exposes_recovery_topology() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        initialize(&pool).await.unwrap();

        let ecosystem = ecosystem(&pool, "ghf-studios", "vapor")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ecosystem.repositories.len(), 2,);

        assert_eq!(ecosystem.repositories[0].name, "Vapor-Root",);

        assert_eq!(ecosystem.repositories[1].name, "Vapor-Server-Root",);

        let account = provider_account(&pool, "github", "GHF-Studios")
            .await
            .unwrap()
            .unwrap();

        assert!(account.account.verified,);

        assert_eq!(account.repositories.len(), 5,);
    }
}
