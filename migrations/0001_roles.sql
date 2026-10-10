-- 0001_roles.sql — the privilege model.
--
-- This migration deliberately creates no role with a password. A migration is a file in the
-- repository, and a password in the repository is a password that has leaked (N-40). Role
-- creation therefore happens at provisioning time, where the secret is in the environment:
--
--   container      deploy/initdb/10-roles.sh, on first initialisation of the data directory
--   local cluster  deploy/bootstrap-local.sql, run once by a superuser
--
-- What belongs here is what follows from the schema rather than from a secret: that the role
-- exists at all, and what it may do to tables created later.

-- Fail early and legibly. Without this check, the first symptom of a skipped bootstrap is a
-- permission error on an unrelated query, hours later and nowhere near the cause.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'watcher_app') THEN
    RAISE EXCEPTION 'role watcher_app does not exist: run the provisioning step before migrating (deploy/initdb/10-roles.sh in the container, deploy/bootstrap-local.sql locally)';
  END IF;
END $$;

-- current_database() rather than a literal, so the migration does not quietly assume the
-- database is called the_watcher.
DO $$
BEGIN
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO watcher_app', current_database());
END $$;

GRANT USAGE ON SCHEMA public TO watcher_app;

-- Tables the owner creates from here on become readable and insertable by the application
-- without anyone remembering to grant them.
--
-- UPDATE and DELETE are deliberately absent, and that is the whole point of doing it this
-- way. The append-only tables must never have them (D-12), and the few tables that genuinely
-- need them are granted explicitly in 0009. Default privileges including UPDATE would invert
-- the failure mode: forgetting would hand out too much. As written, forgetting hands out too
-- little, which fails loudly in a test instead of silently in production.
ALTER DEFAULT PRIVILEGES FOR ROLE watcher_owner IN SCHEMA public
  GRANT SELECT, INSERT ON TABLES TO watcher_app;

ALTER DEFAULT PRIVILEGES FOR ROLE watcher_owner IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO watcher_app;

-- The application never creates objects; the owner does, through migrations. Postgres 15 and
-- later already withhold CREATE on public from PUBLIC, so this is a no-op on a current
-- cluster and a correction on an older one.
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
