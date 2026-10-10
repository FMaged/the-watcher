-- Provisioning for a local cluster, the counterpart to deploy/initdb/10-roles.sh. Creates the
-- two roles and the database that migrations then build on.
--
-- Run once, as a superuser, passing the passwords rather than editing them in:
--
--   sudo -u postgres psql \
--     -v owner_pw="$(openssl rand -base64 24)" \
--     -v app_pw="$(openssl rand -base64 24)" \
--     -f deploy/bootstrap-local.sql
--
-- Then put those same two passwords into deploy/.env as MIGRATE_DATABASE_URL and
-- DATABASE_URL. Nothing here reads or writes that file: a password this script knows is a
-- password you chose, not one the repository stores (N-40).
--
-- Idempotent, so re-running it is a password reset rather than an error.

\set ON_ERROR_STOP on

-- \gexec runs the string the query returns, which is how a conditional CREATE is written in
-- psql: CREATE ROLE and CREATE DATABASE have no IF NOT EXISTS.
SELECT 'CREATE ROLE watcher_owner LOGIN'
 WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'watcher_owner')\gexec
ALTER ROLE watcher_owner LOGIN PASSWORD :'owner_pw';

SELECT 'CREATE ROLE watcher_app LOGIN'
 WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'watcher_app')\gexec
ALTER ROLE watcher_app LOGIN PASSWORD :'app_pw';

SELECT 'CREATE DATABASE the_watcher OWNER watcher_owner'
 WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'the_watcher')\gexec

\echo 'Roles watcher_owner and watcher_app and database the_watcher are provisioned.'
\echo 'Apply migrations as watcher_owner, then verify with deploy/verify-roles.sql.'
