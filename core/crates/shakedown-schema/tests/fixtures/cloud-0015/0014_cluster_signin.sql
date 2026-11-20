-- Signing in to clusters: OpenSSH user certificates (NERSC sshproxy, step-ca and the like) with the key
-- they certify, and how long a signed-in connection is kept open so MFA is needed once, not per restart.
ALTER TABLE clusters DROP CONSTRAINT clusters_auth_check;
ALTER TABLE clusters ADD CONSTRAINT clusters_auth_check CHECK (auth IN ('generated','key','password','certificate'));
-- The certificate is sealed like the key; its principals and end are kept apart, to show on the card.
ALTER TABLE clusters ADD COLUMN sealed_cert bytea;
ALTER TABLE clusters ADD COLUMN cert_principals text[] NOT NULL DEFAULT '{}';
ALTER TABLE clusters ADD COLUMN cert_key_id text;
ALTER TABLE clusters ADD COLUMN cert_valid_before timestamptz;
ALTER TABLE clusters ADD COLUMN hold_minutes integer NOT NULL DEFAULT 720 CHECK (hold_minutes BETWEEN 0 AND 10080);
-- The ops viewer has column grants on clusters: it sees the new plain columns, never sealed_cert.
DO $$
BEGIN
  IF EXISTS (SELECT FROM pg_roles WHERE rolname = 'ops_readonly') THEN
    GRANT SELECT (cert_principals, cert_key_id, cert_valid_before, hold_minutes) ON clusters TO ops_readonly;
  END IF;
END $$;
