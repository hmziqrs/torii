ALTER TABLE requests
ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'http';

ALTER TABLE requests
ADD COLUMN protocol_config_json TEXT NOT NULL DEFAULT '{"version":1,"kind":"http"}';

CREATE INDEX IF NOT EXISTS idx_requests_protocol_kind
ON requests (protocol_kind);

ALTER TABLE history_index
ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'http';

ALTER TABLE history_index
ADD COLUMN request_name TEXT;

ALTER TABLE history_index
ADD COLUMN request_collection_id TEXT;

ALTER TABLE history_index
ADD COLUMN request_parent_folder_id TEXT;

ALTER TABLE history_index
ADD COLUMN request_snapshot_json TEXT;

ALTER TABLE history_index
ADD COLUMN request_snapshot_blob_hash TEXT;

ALTER TABLE history_index
ADD COLUMN run_summary_json TEXT;

ALTER TABLE history_index
ADD COLUMN transcript_blob_hash TEXT;

ALTER TABLE history_index
ADD COLUMN transcript_size INTEGER;

ALTER TABLE history_index
ADD COLUMN message_count_in INTEGER;

ALTER TABLE history_index
ADD COLUMN message_count_out INTEGER;

ALTER TABLE history_index
ADD COLUMN close_reason TEXT;

-- Normalize pre-Phase-5 second precision timestamps into milliseconds.
-- Match normalize_unix_ms threshold (< 1_000_000_000_000 by absolute value).
UPDATE history_index
SET started_at = started_at * 1000
WHERE started_at IS NOT NULL
  AND ABS(started_at) < 1000000000000;

UPDATE history_index
SET completed_at = completed_at * 1000
WHERE completed_at IS NOT NULL
  AND ABS(completed_at) < 1000000000000;

UPDATE history_index
SET dispatched_at = dispatched_at * 1000
WHERE dispatched_at IS NOT NULL
  AND ABS(dispatched_at) < 1000000000000;

UPDATE history_index
SET first_byte_at = first_byte_at * 1000
WHERE first_byte_at IS NOT NULL
  AND ABS(first_byte_at) < 1000000000000;

UPDATE history_index
SET cancelled_at = cancelled_at * 1000
WHERE cancelled_at IS NOT NULL
  AND ABS(cancelled_at) < 1000000000000;

CREATE INDEX IF NOT EXISTS idx_history_workspace_started_id
ON history_index (workspace_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_protocol_started
ON history_index (workspace_id, protocol_kind, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_state_started
ON history_index (workspace_id, state, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_request_started
ON history_index (request_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_status_started
ON history_index (workspace_id, status_code, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_collection_started
ON history_index (workspace_id, request_collection_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_history_workspace_method_started
ON history_index (workspace_id, method COLLATE NOCASE, started_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS history_blob_refs (
    history_id TEXT NOT NULL REFERENCES history_index (id) ON DELETE CASCADE,
    blob_hash TEXT NOT NULL,
    ref_kind TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (history_id, blob_hash, ref_kind)
);

CREATE INDEX IF NOT EXISTS idx_history_blob_refs_blob_hash
ON history_blob_refs (blob_hash);
