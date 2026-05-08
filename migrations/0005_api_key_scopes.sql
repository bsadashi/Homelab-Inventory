-- Per-API-key scope. Previously each key inherited the full role of
-- its owner, so a key issued to an Operator user automatically had
-- Operator privileges across every endpoint. That's fine for trusted
-- service accounts but means a script intended for read-only KPI
-- scraping could mutate inventory if its key leaked.
--
-- The scope column lets the issuer cap a key below its user's role:
--   * 'inherit' — legacy behaviour; key gets the user's role.
--   * 'viewer'  — read-only regardless of the user's role.
--   * 'operator'— write-but-not-admin; clamped to user's role
--                 (a Viewer user can't issue an Operator-scoped key).
--
-- Existing rows default to 'inherit' so behaviour is unchanged for
-- already-issued keys.

ALTER TABLE api_keys ADD COLUMN scope TEXT NOT NULL DEFAULT 'inherit';
