# Vapor Registry Server

Registry service for Vapor ecosystem/source topology and Project metadata, with
the future typed-content catalog boundary defined separately.

The Registry does not replace Git, GitHub, Steam, or Vapor Identity.

The current service is a source-topology registry, not the planned Vapor
content catalog. It does not yet store typed content identities, Workshop
bindings, discovery metadata, release channels, or publishing decisions.

Its current responsibilities are deliberately small:

- register external provider accounts that own Vapor resources;
- register Vapor-compatible repositories;
- identify top-level repositories required to reconstruct an ecosystem;
- expose those facts through a public read API;
- own Project metadata and expose it through membership-gated account APIs.

Git remains authoritative for source and source history.

The public `ghf-studios/vapor` ecosystem includes Vapor Client and Platform
Server source repositories. It does not publish, require, or acquire Loo-Cast.

Protected ecosystem metadata is checked through Identity before Registry
returns it. Configure `VAPOR_REGISTRY_IDENTITY_AUTHZ_URL` to the loopback
Identity authorization route and set `VAPOR_REGISTRY_IDENTITY_AUTHZ_TOKEN` to
the same 64-character hexadecimal server-only token as Identity. Missing or
invalid authorization configuration leaves public records available and
protected records hidden. The Registry never receives the Steam publisher key.

Git repository state such as `.gitmodules` remains authoritative for Container Repo → Workspace revision topology.

Vapor Identity remains authoritative for people, sessions, linked personal provider identities, and roles.

## Current API

```text
GET /healthz
GET /v1/status
GET /v1/ecosystems/{namespace}/{name}
GET /v1/providers/{provider}/{login}
GET /admin/registry (root-authorized read-only ecosystem/source inventory)
```

## Account Project APIs

Project identity is separate from ecosystems and repository/source topology.
Registry owns metadata; Identity owns per-Project membership. The idempotent
bootstrap adds active `ghf-studios/loo-cast` (Loo Cast) and
`ghf-studios/mad-max-modding` (Mad Max Modding) Projects without attaching source
repositories, granting memberships, or changing the ecosystem API.

- `GET /v1/me/projects` returns
  `{ "projects": [{ "id", "namespace", "slug", "display_name", "summary", "state", "role" }] }`.
  Only active Registry records matching Identity's current memberships appear.
  An authenticated account with no matching Projects receives an empty array.
- `GET /v1/projects/{namespace}/{slug}` returns
  `{ "project": { "id", "namespace", "slug", "display_name", "summary", "state" }, "membership": { "role" } }`
  after a fresh Identity check at minimum role `viewer`. Roles are exactly
  `viewer`, `developer`, and `owner`; global/root roles grant no membership.

Both APIs read only the `vapor_user_session` cookie. Operator cookies and
browser bearer headers cannot authorize them. Every response uses
`Cache-Control: private, no-store`; missing/invalid sessions return 401,
unknown/archived Projects and absent membership return the same generic 404,
and Identity configuration, credential, outage, or protocol failures return
503. Membership decisions are never cached between requests.

The existing server-only `VAPOR_REGISTRY_IDENTITY_AUTHZ_URL` must end in
`/v1/internal/authorization/check`. Project requests derive sibling
`/v1/internal/projects/list` and `/check` paths, preserving an optional monolith
`/api/identity` prefix. Only loopback HTTP is accepted; redirects and proxies
are disabled. Requests use the existing dedicated
`VAPOR_REGISTRY_IDENTITY_AUTHZ_TOKEN`, bearer authentication, and audience
`vapor-registry`. Identity must distinguish invalid sessions (401) from bad
Registry credentials (403). No browser receives the service token.

Project namespace and slug are lowercase ASCII letters, digits, `_` or `-`,
1–63 bytes each. IDs are exactly `namespace/slug`. Metadata is JSON data;
browser consumers must render labels and summaries as text. Identity responses
are bounded to 64 KiB and 512 memberships and invalid/duplicate IDs, roles, or
expired decisions fail closed. Project rows live in Registry's SQLite file and
are preserved by composed SQLite backup/restore; this adds no row import API.
