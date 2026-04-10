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
