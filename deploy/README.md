# Deploy

## What runs here

| Service | Role |
|---|---|
| `postgres` | The single control store (ADR 0001) |
| `migrate` | Applies database migrations, then exits |
| `control-api` | The Rust API |
| `web-console` | The SolidJS console, served by nginx |
| `sandbox-runner` | The only component with Docker socket access |
| `gateway` | The OpenShell-facing gateway image |

## Secrets

Secrets are **files**, not environment variables, because environment variables
appear in `docker inspect` and in crash dumps.

```bash
mkdir -p secrets
chmod 0700 secrets
printf '%s' "$(openssl rand -base64 32)" > secrets/postgres_password
PGPW="$(cat secrets/postgres_password)"
# The API runs as the least-privilege role, so row-level security applies to it.
printf '%s' "postgres://nestbox_app:${PGPW}@postgres:5432/nestbox" > secrets/database_url
# Migrations need the schema owner. This file is mounted only into `migrate`.
printf '%s' "postgres://nestbox:${PGPW}@postgres:5432/nestbox" > secrets/database_url_admin
chmod 0600 secrets/*
```

The API reads `DATABASE_URL_FILE` rather than `DATABASE_URL`, so the connection
string never becomes a process argument.

The two URLs are the reason row-level security is real rather than decorative:
`nestbox_app` does not own the tables, so PostgreSQL enforces the tenant policy
on it. `nestbox` is the owner and is used only by migrations and by the
cross-tenant maintenance jobs.

Provider credentials are **not** placed in the compose file. They live in
`secrets/` and the database holds only a `locator` pointing at them (ADR 0005).

## Start

```bash
docker compose up -d --build
docker compose logs -f control-api
```

The console is on `:8080`. The API is not published on the host: the console
proxies it, so there is one entry point and one place to add TLS later.

## First start

The migration seeds:

- one organization and one environment named `default`
- the model catalog
- the operational schedules (sync, reconciliation, retention, expiry checks)
- one admin user, whose initial password is printed once to the migration log

Change that password immediately. It is not stored anywhere retrievable.

## Verify

```bash
curl -fsS http://localhost:8080/api/v1/healthz
curl -fsS http://localhost:8080/api/v1/readyz
```

`readyz` checks the database connection, not just that the process is alive.

## Rebuild from scratch

The development environment is disposable (ADR 0001). To start over:

```bash
docker compose down -v      # -v drops the database volume
docker compose up -d --build
```

## Backup

```bash
docker compose exec -T postgres \
  pg_dump -U nestbox -Fc nestbox > nestbox-$(date +%F).dump
```

Restore with `pg_restore`. Secrets are backed up separately: they are files, and
a database backup deliberately does not contain them.

## Upgrading

Migrations run before the API starts, so an upgrade is:

```bash
git pull
docker compose up -d --build
```

`migrate` applies forward-only migrations. A failed migration stops the rollout
rather than leaving a half-upgraded schema.

## Tests

`docker-compose.test.yml` starts a throwaway PostgreSQL for the integration
tests:

```bash
docker compose -f docker-compose.test.yml up -d
export DATABASE_URL=postgres://nestbox:test@localhost:55432/nestbox_test
cargo test
```
