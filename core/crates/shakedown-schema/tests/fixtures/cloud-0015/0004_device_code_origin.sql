-- When and from where a pairing code was requested, shown on the approval page (against approving someone else's code).
ALTER TABLE device_codes ADD COLUMN requested_at timestamptz NOT NULL DEFAULT now();
ALTER TABLE device_codes ADD COLUMN requested_from text NOT NULL DEFAULT '';
