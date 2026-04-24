-- 0001_init.sql: initial schema for evolve-storage
-- All ids stored as TEXT (UUID hyphenated) except adapter_id which is free-form TEXT.
-- All timestamps stored as TEXT (ISO 8601 UTC).

PRAGMA foreign_keys = ON;

CREATE TABLE projects (
    id                   TEXT PRIMARY KEY NOT NULL,
    adapter_id           TEXT NOT NULL,
    root_path            TEXT NOT NULL UNIQUE,
    name                 TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    champion_config_id   TEXT,
    FOREIGN KEY (champion_config_id) REFERENCES agent_configs(id) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE agent_configs (
    id             TEXT PRIMARY KEY NOT NULL,
    project_id     TEXT NOT NULL,
    adapter_id     TEXT NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('champion','challenger','historical')),
    fingerprint    INTEGER NOT NULL,
    payload_json   TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE INDEX idx_agent_configs_project_role_created
    ON agent_configs(project_id, role, created_at DESC);

CREATE TABLE experiments (
    id                      TEXT PRIMARY KEY NOT NULL,
    project_id              TEXT NOT NULL,
    champion_config_id      TEXT NOT NULL,
    challenger_config_id    TEXT NOT NULL,
    status                  TEXT NOT NULL CHECK (status IN ('running','promoted','aborted','held')),
    traffic_share           REAL NOT NULL CHECK (traffic_share >= 0.0 AND traffic_share <= 1.0),
    started_at              TEXT NOT NULL,
    decided_at              TEXT,
    decision_posterior      REAL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (champion_config_id) REFERENCES agent_configs(id),
    FOREIGN KEY (challenger_config_id) REFERENCES agent_configs(id)
);
-- At most one running experiment per project (partial unique index).
CREATE UNIQUE INDEX uniq_running_experiment_per_project
    ON experiments(project_id)
    WHERE status = 'running';
CREATE INDEX idx_experiments_project_status
    ON experiments(project_id, status);

CREATE TABLE sessions (
    id                      TEXT PRIMARY KEY NOT NULL,
    project_id              TEXT NOT NULL,
    experiment_id           TEXT,
    variant                 TEXT NOT NULL CHECK (variant IN ('champion','challenger')),
    config_id               TEXT NOT NULL,
    started_at              TEXT NOT NULL,
    ended_at                TEXT NOT NULL,
    adapter_session_ref     TEXT,
    FOREIGN KEY (project_id)    REFERENCES projects(id)      ON DELETE CASCADE,
    FOREIGN KEY (experiment_id) REFERENCES experiments(id)   ON DELETE SET NULL,
    FOREIGN KEY (config_id)     REFERENCES agent_configs(id)
);
CREATE INDEX idx_sessions_project_started
    ON sessions(project_id, started_at DESC);
CREATE INDEX idx_sessions_experiment
    ON sessions(experiment_id);

CREATE TABLE signals (
    id              TEXT PRIMARY KEY NOT NULL,
    session_id      TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('explicit','implicit')),
    source          TEXT NOT NULL,
    value           REAL NOT NULL CHECK (value >= 0.0 AND value <= 1.0),
    recorded_at     TEXT NOT NULL,
    payload_json    TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_signals_session ON signals(session_id);
