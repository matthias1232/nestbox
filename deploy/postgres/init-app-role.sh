#!/bin/sh
# Creates the least-privilege application role.
#
# Migrations and maintenance connect as the schema owner; the API connects as
# `nestbox_app`, a non-owner, so row-level security actually applies to it.
# Without this split the API would run as the owner and RLS would be decorative.
#
# The password is read from the mounted secret, never from an environment
# variable, so it does not appear in `docker inspect`.

set -eu

# The postgres entrypoint reads POSTGRES_PASSWORD_FILE as root and exports the
# value, so it is available here even though the file itself is not readable by
# the postgres user.
APP_PASSWORD="${POSTGRES_PASSWORD:?POSTGRES_PASSWORD must be set}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<SQL
DO \$\$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'nestbox_app') THEN
        CREATE ROLE nestbox_app LOGIN PASSWORD '${APP_PASSWORD}';
    ELSE
        ALTER ROLE nestbox_app WITH LOGIN PASSWORD '${APP_PASSWORD}';
    END IF;
END
\$\$;

GRANT CONNECT ON DATABASE ${POSTGRES_DB} TO nestbox_app;
GRANT USAGE ON SCHEMA public TO nestbox_app;
SQL

echo "nestbox_app role ready"
