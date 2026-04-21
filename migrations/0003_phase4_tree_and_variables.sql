ALTER TABLE collections
ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'managed';

ALTER TABLE collections
ADD COLUMN storage_config_json TEXT NOT NULL DEFAULT '{}';

ALTER TABLE workspaces
ADD COLUMN variables_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE requests
ADD COLUMN variable_overrides_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE environments
ADD COLUMN collection_id TEXT;

UPDATE environments
SET collection_id = (
    SELECT c.id
    FROM collections c
    WHERE c.workspace_id = environments.workspace_id
    ORDER BY c.sort_order ASC, c.id ASC
    LIMIT 1
)
WHERE collection_id IS NULL;

CREATE TABLE environments_new (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    variables_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

INSERT INTO environments_new (
    id,
    collection_id,
    name,
    variables_json,
    created_at,
    updated_at,
    revision
)
SELECT
    id,
    collection_id,
    name,
    CASE
        WHEN json_valid(variables_json) THEN variables_json
        ELSE '{}'
    END,
    created_at,
    updated_at,
    revision
FROM environments
WHERE collection_id IS NOT NULL;

DROP TABLE environments;
ALTER TABLE environments_new RENAME TO environments;

CREATE INDEX IF NOT EXISTS idx_environments_collection
ON environments (collection_id);

CREATE TABLE IF NOT EXISTS tab_session_workspace_state (
    session_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    active_environment_id TEXT,
    expanded_items_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (session_id, workspace_id)
);
