CREATE TABLE IF NOT EXISTS tab_session_metadata (
    session_id TEXT PRIMARY KEY,
    selected_workspace_id TEXT,
    sidebar_item_kind TEXT,
    sidebar_item_id TEXT,
    sidebar_collapsed INTEGER NOT NULL DEFAULT 0,
    sidebar_width_px REAL NOT NULL DEFAULT 255,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_tab_session_metadata_updated_at
    ON tab_session_metadata(updated_at DESC);
