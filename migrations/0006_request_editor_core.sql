-- Phase 3: Expand requests table with structured editor sections.
-- Additive ALTER TABLE only; defaults allow existing rows to load through map_request_row.

ALTER TABLE requests ADD COLUMN params_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE requests ADD COLUMN headers_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE requests ADD COLUMN auth_json TEXT NOT NULL DEFAULT '{"type":"none"}';
ALTER TABLE requests ADD COLUMN body_json TEXT NOT NULL DEFAULT '{"type":"none"}';
ALTER TABLE requests ADD COLUMN scripts_json TEXT NOT NULL DEFAULT '{"pre_request":"","tests":""}';
ALTER TABLE requests ADD COLUMN settings_json TEXT NOT NULL DEFAULT '{}';
