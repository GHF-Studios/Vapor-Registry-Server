# Vapor Registry Server

Registry service for Vapor ecosystem/source topology today, with the future
typed-content catalog boundary defined separately.

The Registry does not replace Git, GitHub, Steam, or Vapor Identity.

The current service is a source-topology registry, not the planned Vapor
content catalog. It does not yet store typed content identities, Workshop
bindings, discovery metadata, release channels, or publishing decisions.

Its current responsibilities are deliberately small:

- register external provider accounts that own Vapor resources;
- register Vapor-compatible repositories;
- identify top-level repositories required to reconstruct an ecosystem;
- expose those facts through a public read API.

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
