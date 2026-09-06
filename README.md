# Vapor Registry Server

Canonical Registry service for Vapor semantic identity and external-provider linkage.

The Registry does not replace Git, GitHub, Steam, or Vapor Identity.

Its current responsibilities are deliberately small:

- register external provider accounts that own Vapor resources;
- register Vapor-compatible repositories;
- identify top-level repositories required to reconstruct an ecosystem;
- expose those facts through a public read API.

Git remains authoritative for source and source history.

Git repository state such as `.gitmodules` remains authoritative for Container Repo → Workspace revision topology.

Vapor Identity remains authoritative for people, sessions, linked personal provider identities, and roles.

## Current API

```text
GET /healthz
GET /v1/status
GET /v1/ecosystems/{namespace}/{name}
GET /v1/providers/{provider}/{login}