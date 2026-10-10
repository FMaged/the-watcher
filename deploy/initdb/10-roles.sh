#!/bin/sh
# Runs once, on first initialisation of the data directory, as the superuser the postgres
# image creates. This is where the application role gets its password, because here the
# secret is in the environment rather than in a file in the repository (N-40).
#
# Migration 0001 then grants that role its privileges. The order matters: this script runs at
# container creation, long before the binary applies any migration.
set -eu

: "${WATCHER_APP_PASSWORD:?set WATCHER_APP_PASSWORD in deploy/.env}"

# -v passes the password as a psql variable so that :'app_pw' expands to a properly quoted
# literal. Interpolating it into the SQL text with the shell would break on any password
# containing a quote, and would break in a way that looked like a syntax error.
psql -v ON_ERROR_STOP=1 -v app_pw="$WATCHER_APP_PASSWORD" \
     --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'watcher_app') THEN
    CREATE ROLE watcher_app LOGIN;
  END IF;
END $$;

ALTER ROLE watcher_app LOGIN PASSWORD :'app_pw';
SQL
