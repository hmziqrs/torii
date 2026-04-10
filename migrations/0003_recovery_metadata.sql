ALTER TABLE history_index
ADD COLUMN recovery_attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE history_index
ADD COLUMN finalized_at INTEGER;

CREATE TABLE IF NOT EXISTS startup_recovery_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    stale_temp_removed INTEGER NOT NULL DEFAULT 0,
    orphan_blob_removed INTEGER NOT NULL DEFAULT 0,
    pending_history_failed INTEGER NOT NULL DEFAULT 0
);
