-- Counts can now record their provenance. Existing rows default to
-- 'manual' so behaviour is unchanged for already-recorded counts;
-- vision-derived counts pushed by the companion service set
-- source='vision' and may attach a confidence + evidence URL
-- (often a snapshot of the camera frame the model classified).

ALTER TABLE counts ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE counts ADD COLUMN confidence REAL;
ALTER TABLE counts ADD COLUMN evidence_url TEXT;
