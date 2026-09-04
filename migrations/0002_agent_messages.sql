-- Durable agent sessions: rollout messages per thread (plan §6, resume support).
CREATE TABLE IF NOT EXISTS agent_messages (
    thread_id     TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    role          TEXT NOT NULL,
    content_json  TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
);
