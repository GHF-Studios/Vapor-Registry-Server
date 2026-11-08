use crate::authorization::{LocalSourceRequirement, PermissionPolicy};
use crate::types::{
    AdminEcosystemRow, Ecosystem, EcosystemRepository, EcosystemResponse, ProviderAccount,
    ProviderAccountResponse, ProviderRepository, RegistryStatus, REGISTRY_SCHEMA_VERSION,
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
    validate_access_policies(pool).await?;

    Ok(())
}

async fn validate_access_policies(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT schema_version, visibility, permission_policy_json,
                local_requirements_json
         FROM ecosystem_access_policies",
    )
    .fetch_all(pool)
    .await?;

    for (schema_version, visibility, permission_json, local_json) in rows {
        let invalid = || sqlx::Error::Protocol("invalid ecosystem access policy".into());
        if schema_version != 1 || !matches!(visibility.as_str(), "public" | "protected") {
            return Err(invalid());
        }
        let permission_policy: PermissionPolicy =
            serde_json::from_str(&permission_json).map_err(|_| invalid())?;
        if permission_policy.requirements.len() > 16
            || (visibility == "protected" && permission_policy.requirements.is_empty())
        {
            return Err(invalid());
        }
        let local_requirements: Vec<LocalSourceRequirement> =
            serde_json::from_str(&local_json).map_err(|_| invalid())?;
        if local_requirements
            .iter()
            .any(|requirement| requirement.source_id.trim().is_empty())
        {
            return Err(invalid());
        }
    }

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
        CREATE TABLE IF NOT EXISTS ecosystem_access_policies (
            ecosystem_id TEXT PRIMARY KEY
                REFERENCES ecosystems(id)
                ON DELETE CASCADE,
            schema_version INTEGER NOT NULL,
            visibility TEXT NOT NULL CHECK (visibility IN ('public', 'protected')),
            permission_policy_json TEXT NOT NULL,
            local_requirements_json TEXT NOT NULL
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

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO ecosystem_access_policies(
            ecosystem_id, schema_version, visibility,
            permission_policy_json, local_requirements_json
        ) VALUES (?, 1, 'public', '{"mode":"all","requirements":[]}', '[]')
        "#,
    )
    .bind(VAPOR_ECOSYSTEM_ID)
    .execute(&mut *transaction)
    .await?;

    let repositories = [
        (
            "github:GHF-Studios/Vapor-Client",
            "Vapor-Client",
            "container-repo",
            "https://github.com/GHF-Studios/Vapor-Client",
            "https://github.com/GHF-Studios/Vapor-Client.git",
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
            "github:GHF-Studios/Vapor-Platform-Server",
            "Vapor-Platform-Server",
            "container-repo",
            "https://github.com/GHF-Studios/Vapor-Platform-Server",
            "https://github.com/GHF-Studios/Vapor-Platform-Server.git",
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

    // Remove the legacy public listing as well as its ecosystem membership.
    // This migration is intentionally idempotent so existing installations
    // stop exposing the Loo-Cast repository after upgrading the Registry.
    sqlx::query("DELETE FROM repositories WHERE id = 'github:GHF-Studios/Loo-Cast'")
        .execute(&mut *transaction)
        .await?;

    // The public Vapor ecosystem is intentionally usable without Loo-Cast.
    // Container Repos remain authoritative for their contained Workspace
    // revisions through authored `.gitmodules`.
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
        ("github:GHF-Studios/Vapor-Client", "Vapor-Client", 10_i64),
        (
            "github:GHF-Studios/Vapor-Platform-Server",
            "Vapor-Platform-Server",
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

pub async fn admin_ecosystems(pool: &SqlitePool) -> Result<Vec<AdminEcosystemRow>, sqlx::Error> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT
            ecosystems.id,
            ecosystems.display_name,
            ecosystem_access_policies.visibility,
            provider_accounts.provider,
            provider_accounts.login,
            repositories.name,
            repositories.kind,
            ecosystem_repositories.checkout_path
        FROM ecosystems
        LEFT JOIN ecosystem_access_policies
          ON ecosystem_access_policies.ecosystem_id = ecosystems.id
        LEFT JOIN ecosystem_repositories
          ON ecosystem_repositories.ecosystem_id = ecosystems.id
        LEFT JOIN repositories
          ON repositories.id = ecosystem_repositories.repository_id
        LEFT JOIN provider_accounts
          ON provider_accounts.id = repositories.provider_account_id
        ORDER BY ecosystems.id, ecosystem_repositories.ordinal, repositories.id
        LIMIT 200
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                ecosystem_id,
                display_name,
                visibility,
                provider,
                owner,
                repository,
                source_kind,
                checkout_path,
            )| AdminEcosystemRow {
                ecosystem_id,
                display_name,
                visibility: visibility.unwrap_or_else(|| "unknown".to_owned()),
                provider,
                owner,
                repository,
                source_kind,
                checkout_path,
            },
        )
        .collect())
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
        local_requirements: Vec::new(),
    }))
}

pub async fn ecosystem_access_policy(
    pool: &SqlitePool,
    ecosystem_id: &str,
) -> Result<Option<(i64, String, String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT schema_version, visibility, permission_policy_json,
                local_requirements_json
         FROM ecosystem_access_policies
         WHERE ecosystem_id = ?",
    )
    .bind(ecosystem_id)
    .fetch_optional(pool)
    .await
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

        assert_eq!(ecosystem.repositories[0].name, "Vapor-Client",);

        assert_eq!(ecosystem.repositories[1].name, "Vapor-Platform-Server",);

        assert!(!ecosystem
            .repositories
            .iter()
            .any(|repository| repository.name == "Loo-Cast"));

        let account = provider_account(&pool, "github", "GHF-Studios")
            .await
            .unwrap()
            .unwrap();

        assert!(account.account.verified,);

        assert_eq!(account.repositories.len(), 5,);

        assert!(!account
            .repositories
            .iter()
            .any(|repository| repository.name == "Loo-Cast"));
    }
}
