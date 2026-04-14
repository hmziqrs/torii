CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    parent_folder_id TEXT REFERENCES folders (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS requests (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    parent_folder_id TEXT REFERENCES folders (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    method TEXT NOT NULL DEFAULT 'GET',
    url TEXT NOT NULL DEFAULT '',
    body_blob_hash TEXT,
    sort_order INTEGER NOT NULL,
    params_json TEXT NOT NULL DEFAULT '[]',
    headers_json TEXT NOT NULL DEFAULT '[]',
    auth_json TEXT NOT NULL DEFAULT '{"type":"none"}',
    body_json TEXT NOT NULL DEFAULT '{"type":"none"}',
    scripts_json TEXT NOT NULL DEFAULT '{"pre_request":"","tests":""}',
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS environments (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    variables_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ui_preferences (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS history_index (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    request_id TEXT REFERENCES requests (id) ON DELETE SET NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    status_code INTEGER,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    state TEXT NOT NULL,
    blob_hash TEXT,
    blob_size INTEGER,
    error_message TEXT,
    recovery_attempts INTEGER NOT NULL DEFAULT 0,
    finalized_at INTEGER,
    response_headers_json TEXT,
    response_media_type TEXT,
    dispatched_at INTEGER,
    first_byte_at INTEGER,
    cancelled_at INTEGER,
    partial_size INTEGER,
    request_url_redacted TEXT,
    request_headers_redacted_json TEXT,
    request_auth_kind TEXT,
    request_body_summary_json TEXT,
    request_method TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS secret_refs (
    id TEXT PRIMARY KEY,
    owner_kind TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    secret_kind TEXT NOT NULL,
    provider TEXT NOT NULL,
    namespace TEXT NOT NULL,
    key_name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (owner_kind, owner_id, secret_kind)
);

CREATE TABLE IF NOT EXISTS startup_recovery_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    stale_temp_removed INTEGER NOT NULL DEFAULT 0,
    orphan_blob_removed INTEGER NOT NULL DEFAULT 0,
    pending_history_failed INTEGER NOT NULL DEFAULT 0
);

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

CREATE INDEX IF NOT EXISTS idx_collections_workspace_sort
ON collections (workspace_id, sort_order);

CREATE INDEX IF NOT EXISTS idx_folders_collection_parent_sort
ON folders (collection_id, parent_folder_id, sort_order);

CREATE INDEX IF NOT EXISTS idx_requests_collection_parent_sort
ON requests (collection_id, parent_folder_id, sort_order);

CREATE INDEX IF NOT EXISTS idx_environments_workspace
ON environments (workspace_id);

CREATE INDEX IF NOT EXISTS idx_history_workspace_started
ON history_index (workspace_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_history_state
ON history_index (state, updated_at);

CREATE INDEX IF NOT EXISTS idx_history_blob_hash
ON history_index (blob_hash);

CREATE INDEX IF NOT EXISTS idx_secret_owner
ON secret_refs (owner_kind, owner_id);

CREATE INDEX IF NOT EXISTS idx_tab_session_updated
ON tab_session_state (updated_at DESC, session_id);

CREATE INDEX IF NOT EXISTS idx_tab_session_metadata_updated_at
    ON tab_session_metadata(updated_at DESC);
