-- Phase 3: Expand history_index with response metadata for the latest-run summary.
-- Required for finalize_cancelled and richer response display.

ALTER TABLE history_index ADD COLUMN response_headers_json TEXT;
ALTER TABLE history_index ADD COLUMN response_media_type TEXT;
ALTER TABLE history_index ADD COLUMN dispatched_at INTEGER;
ALTER TABLE history_index ADD COLUMN first_byte_at INTEGER;
ALTER TABLE history_index ADD COLUMN cancelled_at INTEGER;
ALTER TABLE history_index ADD COLUMN partial_size INTEGER;
