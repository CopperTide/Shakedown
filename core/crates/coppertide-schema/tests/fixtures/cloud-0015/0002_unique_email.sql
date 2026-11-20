-- One account per email address, whatever the sign-in method.
CREATE UNIQUE INDEX users_email_unique ON users (lower(email));
