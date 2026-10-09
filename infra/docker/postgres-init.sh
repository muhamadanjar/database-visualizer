#!/bin/sh
set -eu
psql -v ON_ERROR_STOP=1 -v test_password="$POSTGRES_PASSWORD" --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
CREATE ROLE studio LOGIN PASSWORD :'test_password' NOSUPERUSER;
GRANT ALL ON DATABASE studio_test TO studio;
GRANT ALL ON SCHEMA public TO studio;
SQL
