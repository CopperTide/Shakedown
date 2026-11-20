-- Claiming an old Community account: the signed-in user who started the Google flow.
ALTER TABLE login_flows DROP CONSTRAINT login_flows_purpose_check;
ALTER TABLE login_flows ADD CONSTRAINT login_flows_purpose_check CHECK (purpose IN ('join','login','claim'));
ALTER TABLE login_flows ADD COLUMN user_id uuid REFERENCES users(id) ON DELETE CASCADE;
