CREATE TABLE IF NOT EXISTS tab_session_state (
    session_id TEXT NOT NULL,
    tab_order INTEGER NOT NULL,
    item_kind TEXT NOT NULL,
    item_id TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (session_id, tab_order)
);

CREATE INDEX IF NOT EXISTS idx_tab_session_updated
ON tab_session_state (updated_at DESC, session_id);
