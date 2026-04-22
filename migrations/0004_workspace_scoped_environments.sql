CREATE TABLE environments_new (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    variables_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    revision INTEGER NOT NULL
);

INSERT INTO environments_new (
    id,
    workspace_id,
    name,
    variables_json,
    created_at,
    updated_at,
    revision
)
SELECT
    e.id,
    c.workspace_id,
    e.name,
    CASE
        WHEN json_valid(e.variables_json) THEN e.variables_json
        ELSE '{}'
    END,
    e.created_at,
    e.updated_at,
    e.revision
FROM environments e
INNER JOIN collections c ON c.id = e.collection_id;

DROP TABLE environments;
ALTER TABLE environments_new RENAME TO environments;

CREATE INDEX IF NOT EXISTS idx_environments_workspace
ON environments (workspace_id);
