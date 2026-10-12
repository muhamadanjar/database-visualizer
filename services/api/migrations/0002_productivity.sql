CREATE TABLE saved_query_favorites (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 document_id uuid NOT NULL,
 PRIMARY KEY(organization_id,workspace_id,user_id,document_id),
 FOREIGN KEY(organization_id,workspace_id,document_id) REFERENCES documents(organization_id,workspace_id,id) ON DELETE CASCADE
);
CREATE TABLE mutations (
 organization_id uuid NOT NULL, workspace_id uuid NOT NULL, connection_id uuid NOT NULL, id uuid NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, login_hash bytea NOT NULL, tab_hash bytea NOT NULL,
 idempotency_key text NOT NULL, request_hash bytea NOT NULL, state text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), finished_at timestamptz,
 PRIMARY KEY(organization_id,workspace_id,id), UNIQUE(organization_id,workspace_id,user_id,idempotency_key),
 FOREIGN KEY(organization_id,workspace_id,connection_id) REFERENCES connections(organization_id,workspace_id,id) ON DELETE CASCADE
);
CREATE INDEX mutations_retention ON mutations(created_at);
