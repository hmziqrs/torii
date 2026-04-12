-- Phase 3 follow-up: persist redacted request method in snapshot columns.
ALTER TABLE history_index ADD COLUMN request_method TEXT;
