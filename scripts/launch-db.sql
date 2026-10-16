\set ON_ERROR_STOP on
-- Never put credentials into statement/error logs or command-line arguments.
SET log_statement = 'none';
SET log_min_error_statement = 'panic';
\getenv db_name DB_NAME
\getenv db_user DB_USER
\getenv db_password DB_PASSWORD
SELECT octet_length(:'db_name') BETWEEN 1 AND 63
   AND octet_length(:'db_user') BETWEEN 1 AND 63
   AND octet_length(:'db_password') > 0
   AND NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'db_user'
       AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls)) AS safe_role \gset
\if :safe_role
\else
  DO $$ BEGIN RAISE EXCEPTION 'Refusing an invalid or privileged database role'; END $$;
\endif
SELECT format('CREATE ROLE %I LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE', :'db_user')
 WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'db_user') \gexec
SELECT NOT EXISTS (SELECT 1 FROM pg_database
 WHERE datname = :'db_name' AND pg_get_userbyid(datdba) <> :'db_user') AS safe_owner \gset
\if :safe_owner
\else
  DO $$ BEGIN RAISE EXCEPTION 'Existing database belongs to another role; no ownership changes made'; END $$;
\endif
SELECT format('ALTER ROLE %I LOGIN PASSWORD %L', :'db_user', :'db_password') \gexec
SELECT format('CREATE DATABASE %I OWNER %I', :'db_name', :'db_user')
 WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'db_name') \gexec
