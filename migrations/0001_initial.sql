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
