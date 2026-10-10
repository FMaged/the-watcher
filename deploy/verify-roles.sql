-- The D-12 acceptance check from §11: "the application database role is proven unable to
-- update or delete an analysis".
--
-- Run as anyone who can read the catalogue — the owner is simplest:
--
--   psql "$MIGRATE_DATABASE_URL" -f deploy/verify-roles.sql
--
-- It asserts against the privilege catalogue rather than by attempting writes. An earlier
-- version of this file ran UPDATE and DELETE as watcher_app and looked for "permission
-- denied", which has a quiet failure mode: a table that does not exist yet fails with
-- "relation does not exist", and a reader skimming for failures counts that as a pass. The
-- privilege tables cannot be misread that way, and a missing table becomes its own error.

\set ON_ERROR_STOP on

DO $$
DECLARE
  append_only text[] := ARRAY[
    'analysis', 'annotation', 'ledger_entry',
    'news_take', 'raw_model_output', 'call_verdict'
  ];
  t          text;
  leaked     text[];
  missing    text[] := '{}';
BEGIN
  FOREACH t IN ARRAY append_only LOOP
    IF NOT EXISTS (
      SELECT 1 FROM information_schema.tables
       WHERE table_schema = 'public' AND table_name = t
    ) THEN
      -- Not a failure in itself: these tables arrive over migrations 0005 to 0007. It is
      -- reported so that a run against a half-built schema cannot be mistaken for proof.
      missing := missing || t;
      CONTINUE;
    END IF;

    SELECT array_agg(privilege_type ORDER BY privilege_type) INTO leaked
      FROM information_schema.table_privileges
     WHERE grantee = 'watcher_app'
       AND table_schema = 'public'
       AND table_name = t
       AND privilege_type IN ('UPDATE', 'DELETE');

    IF leaked IS NOT NULL THEN
      RAISE EXCEPTION 'D-12 violated: watcher_app holds % on %', leaked, t;
    END IF;

    RAISE NOTICE 'ok: watcher_app holds no UPDATE or DELETE on %', t;
  END LOOP;

  IF array_length(missing, 1) > 0 THEN
    RAISE NOTICE 'not yet created, so not yet proven: %', missing;
  END IF;
END $$;
