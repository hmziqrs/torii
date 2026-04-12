-- Phase 3: Secret-safe sent-request snapshot columns for history rows.
-- These capture what was sent (redacted) alongside response metadata.

ALTER TABLE history_index ADD COLUMN request_url_redacted TEXT;
ALTER TABLE history_index ADD COLUMN request_headers_redacted_json TEXT;
ALTER TABLE history_index ADD COLUMN request_auth_kind TEXT;
ALTER TABLE history_index ADD COLUMN request_body_summary_json TEXT;
