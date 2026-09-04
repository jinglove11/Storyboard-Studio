-- Storyboard Studio schema v1 (plan §18 Table 15).
-- API keys never live here; providers.config_json holds only a secret
-- *reference* resolved via the OS keychain.

CREATE TABLE IF NOT EXISTS templates (
    id                   TEXT PRIMARY KEY,          -- T001
    title                TEXT NOT NULL,
    current_revision_id  TEXT NOT NULL,
    created_at           TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS template_revisions (
    id                  TEXT PRIMARY KEY,           -- rev_<sha16>
    template_id         TEXT NOT NULL REFERENCES templates(id),
    file_path           TEXT NOT NULL,              -- templates/originals/<sha>.json
    sha256              TEXT NOT NULL UNIQUE,
    schema_fingerprint  TEXT NOT NULL,
    imported_at         TEXT NOT NULL
);

-- Full rebuilt metadata as JSON plus denormalized filter columns.
CREATE TABLE IF NOT EXISTS template_metadata (
    revision_id       TEXT PRIMARY KEY REFERENCES template_revisions(id),
    scene_family      TEXT NOT NULL DEFAULT '',
    exact_scene       TEXT,
    time_tags_json    TEXT NOT NULL DEFAULT '[]',
    panel_count       INTEGER NOT NULL,
    total_role_count  INTEGER NOT NULL,
    female_lead_count INTEGER,
    male_lead_count   INTEGER,
    pace              TEXT NOT NULL DEFAULT 'standard',
    narrative_type    TEXT,
    metadata_json     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS template_tags (
    revision_id  TEXT NOT NULL REFERENCES template_revisions(id),
    kind         TEXT NOT NULL,     -- scene|location|time|environment|camera|composition|interaction|prop|keyword|anchor
    value        TEXT NOT NULL,
    weight       REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (revision_id, kind, value)
);

CREATE TABLE IF NOT EXISTS characters (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    identity_json       TEXT NOT NULL,
    default_outfit_json TEXT,
    created_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
    id                          TEXT PRIMARY KEY,
    title                       TEXT NOT NULL,
    source_template_id          TEXT NOT NULL,
    source_template_revision_id TEXT NOT NULL,
    current_version             INTEGER NOT NULL,
    status                      TEXT NOT NULL,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project_versions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id     TEXT NOT NULL REFERENCES projects(id),
    version_number INTEGER NOT NULL,
    parent_version INTEGER,
    snapshot_path  TEXT NOT NULL,
    diff_path      TEXT,
    created_at     TEXT NOT NULL,
    UNIQUE (project_id, version_number)
);

CREATE TABLE IF NOT EXISTS patches (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id        TEXT NOT NULL REFERENCES projects(id),
    base_version      INTEGER NOT NULL,
    proposal_json     TEXT NOT NULL,
    validation_json   TEXT,
    status            TEXT NOT NULL,  -- proposed|validated|approved|rejected|committed|stale
    run_id            TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_threads (
    id          TEXT PRIMARY KEY,
    project_id  TEXT,
    provider_id TEXT NOT NULL,
    model       TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_events (
    thread_id    TEXT NOT NULL REFERENCES agent_threads(id),
    seq          INTEGER NOT NULL,
    type         TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
);

CREATE TABLE IF NOT EXISTS agent_runs (
    id                      TEXT PRIMARY KEY,
    thread_id               TEXT REFERENCES agent_threads(id),
    provider_id             TEXT NOT NULL,
    model                   TEXT NOT NULL,
    prompt_preset_version   TEXT NOT NULL,
    core_contract_hash      TEXT NOT NULL,
    tool_registry_version   TEXT NOT NULL,
    template_revision_id    TEXT,
    base_project_version_id INTEGER,
    sampling_json           TEXT NOT NULL,
    manifest_json           TEXT NOT NULL,
    created_at              TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
    id          TEXT PRIMARY KEY,
    type        TEXT NOT NULL,
    name        TEXT NOT NULL,
    config_json TEXT NOT NULL,        -- secrets excluded; only references
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_events (
    seq           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind          TEXT NOT NULL,
    payload_json  TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tags_lookup ON template_tags (kind, value);
CREATE INDEX IF NOT EXISTS idx_metadata_scene ON template_metadata (scene_family);
