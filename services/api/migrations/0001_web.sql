CREATE TABLE users (
 id uuid PRIMARY KEY, display_name text NOT NULL CHECK(length(display_name) BETWEEN 1 AND 512),
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE provider_identities (
 provider text NOT NULL CHECK(provider IN ('github','google','test')), subject text NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 PRIMARY KEY(provider,subject)
);
CREATE TABLE organizations (
 id uuid PRIMARY KEY, name text NOT NULL CHECK(length(name) BETWEEN 1 AND 512),
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE organization_members (
 organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 role text NOT NULL CHECK(role IN ('Owner','Member')),
 PRIMARY KEY(organization_id,user_id)
);
CREATE TABLE workspaces (
 organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 id uuid NOT NULL, name text NOT NULL CHECK(length(name) BETWEEN 1 AND 512),
 created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(organization_id,id)
);
CREATE TABLE workspace_members (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, user_id uuid NOT NULL,
 role text NOT NULL CHECK(role IN ('Admin','Developer','Viewer')),
 PRIMARY KEY(organization_id,workspace_id,user_id),
 FOREIGN KEY(organization_id,workspace_id) REFERENCES workspaces(organization_id,id) ON DELETE CASCADE,
 FOREIGN KEY(organization_id,user_id) REFERENCES organization_members(organization_id,user_id) ON DELETE CASCADE
);
CREATE TABLE connections (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, id uuid NOT NULL,
 revision bigint NOT NULL CHECK(revision BETWEEN 1 AND 4294967295),
 profile jsonb NOT NULL, secret bytea NOT NULL,
 PRIMARY KEY(organization_id,workspace_id,id),
 FOREIGN KEY(organization_id,workspace_id) REFERENCES workspaces(organization_id,id) ON DELETE CASCADE
);
CREATE TABLE documents (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, connection_id uuid NOT NULL,
 id uuid NOT NULL, user_id uuid NOT NULL REFERENCES users(id),
 kind text NOT NULL CHECK(kind IN ('saved','history','editor')),
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 512), content bytea NOT NULL,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision > 0), created_at_ms bigint NOT NULL,
 PRIMARY KEY(organization_id,workspace_id,id),
 FOREIGN KEY(organization_id,workspace_id,connection_id) REFERENCES connections(organization_id,workspace_id,id) ON DELETE CASCADE
);
CREATE INDEX documents_scope ON documents(organization_id,workspace_id,connection_id,kind,user_id,created_at_ms DESC,id);
CREATE TABLE settings (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, user_id uuid NOT NULL REFERENCES users(id),
 layout jsonb NOT NULL DEFAULT '{}', PRIMARY KEY(organization_id,workspace_id,user_id),
 FOREIGN KEY(organization_id,workspace_id) REFERENCES workspaces(organization_id,id) ON DELETE CASCADE
);
CREATE TABLE sessions (
 token_hash bytea PRIMARY KEY, user_id uuid NOT NULL REFERENCES users(id),
 csrf_hash bytea NOT NULL, csrf_cipher bytea NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), last_activity timestamptz NOT NULL DEFAULT now(),
 expires_at timestamptz NOT NULL DEFAULT now() + interval '12 hours', revoked boolean NOT NULL DEFAULT false
);
CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE TABLE oauth_states (
 state_hash bytea PRIMARY KEY, provider text NOT NULL, browser_hash bytea NOT NULL,
 verifier bytea NOT NULL, expires_at timestamptz NOT NULL DEFAULT now() + interval '5 minutes'
);
CREATE TABLE executions (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, connection_id uuid NOT NULL,
 id uuid NOT NULL, user_id uuid NOT NULL REFERENCES users(id), session_id uuid NOT NULL,
 login_hash bytea NOT NULL, tab_hash bytea NOT NULL, idempotency_key text NOT NULL CHECK(length(idempotency_key) BETWEEN 1 AND 128),
 request_hash bytea NOT NULL, state text NOT NULL DEFAULT 'Created', outcome text NOT NULL DEFAULT 'Unknown',
 created_at timestamptz NOT NULL DEFAULT now(), finished_at timestamptz,
 PRIMARY KEY(organization_id,workspace_id,id),
 UNIQUE(organization_id,workspace_id,user_id,idempotency_key),
 FOREIGN KEY(organization_id,workspace_id,connection_id) REFERENCES connections(organization_id,workspace_id,id) ON DELETE CASCADE
);
CREATE TABLE audit_events (
 organization_id uuid NOT NULL REFERENCES organizations(id), workspace_id uuid, id uuid PRIMARY KEY,
 user_id uuid NOT NULL, action text NOT NULL, resource_id uuid, outcome text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_scope ON audit_events(organization_id,workspace_id,created_at DESC,id);
