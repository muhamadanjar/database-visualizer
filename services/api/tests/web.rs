#![cfg(feature = "test-provider")]
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sqlx::{ConnectOptions, PgPool, postgres::PgConnectOptions};
use std::{str::FromStr, sync::Arc, time::Duration};
use studio_api::{
    App,
    config::{Config, NetworkPolicy, TargetRule},
    keys::KeyRing,
    store::Store,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;

struct Fixture {
    app: Arc<App>,
    origin: String,
    schema: String,
    admin: PgPool,
    task: tokio::task::JoinHandle<()>,
    _directory: tempfile::TempDir,
}
struct User {
    cookie: String,
    csrf: String,
    id: String,
    tab: String,
}
impl Fixture {
    async fn open() -> Self {
        let database = std::env::var("STUDIO_WEB_TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://studio_app:studio_test@127.0.0.1:55433/studio_app".into()
        });
        let admin = PgPool::connect_with(
            PgConnectOptions::from_str(&database)
                .unwrap()
                .disable_statement_logging(),
        )
        .await
        .expect("disposable application PostgreSQL must be running");
        let schema = format!("web_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let mut url = url::Url::parse(&database).unwrap();
        url.query_pairs_mut()
            .append_pair("options", &format!("-csearch_path={schema}"));
        let options = PgConnectOptions::from_str(url.as_str())
            .unwrap()
            .disable_statement_logging();
        let pool = PgPool::connect_with(options).await.unwrap();
        Store::migrate(&pool).await.unwrap();
        pool.close().await;
        let directory = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let origin = format!("http://{address}");
        let config = Config {
            origin: origin.clone(),
            bind: address,
            database_url: zeroize::Zeroizing::new(url.to_string()),
            keys: Arc::new(KeyRing::from_key("test", [17; 32])),
            spool_directory: directory.path().join("results"),
            network: NetworkPolicy {
                rules: vec![
                    TargetRule {
                        host: "127.0.0.1".into(),
                        port: 55432,
                        network: "127.0.0.1/32".parse().unwrap(),
                    },
                    TargetRule {
                        host: "127.0.0.1".into(),
                        port: 53306,
                        network: "127.0.0.1/32".parse().unwrap(),
                    },
                ],
                protected: vec!["127.0.0.1:55433".parse().unwrap()],
                development: true,
            },
            bootstrap_owner: "test:owner".into(),
            bootstrap_name: "Test organization".into(),
            audit_days: 90,
            github: None,
            google: None,
        };
        let app = App::open(config).await.unwrap();
        app.background();
        let router = studio_api::router(app.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            app,
            origin,
            schema,
            admin,
            task,
            _directory: directory,
        }
    }
    async fn close(self) {
        self.app.shutdown.cancel();
        self.app.close().await.unwrap();
        self.task.abort();
        let _ = self.task.await;

        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
    async fn login(&self, subject: &str) -> User {
        let response = Client::new()
            .post(format!("{}/api/v1/auth/test/login", self.origin))
            .header("Origin", &self.origin)
            .json(&json!({"subject":subject}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let value: Value = response.json().await.unwrap();
        User {
            tab: studio_api::auth::random_token(),
            cookie,
            csrf: value["data"]["csrf_token"].as_str().unwrap().into(),
            id: value["data"]["user_id"].as_str().unwrap().into(),
        }
    }
    async fn call(
        &self,
        user: &User,
        method: Method,
        path: &str,
        body: Value,
        key: Option<&str>,
    ) -> (u16, Value) {
        let mut request = Client::new()
            .request(method, format!("{}{path}", self.origin))
            .header("Cookie", &user.cookie)
            .header("Origin", &self.origin)
            .header("x-csrf-token", &user.csrf)
            .header("x-studio-tab", &user.tab);
        if !body.is_null() {
            request = request.json(&body);
        }
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let value = response.json().await.unwrap();
        (status, value)
    }
    async fn ok(&self, user: &User, method: Method, path: &str, body: Value) -> Value {
        let (status, value) = self.call(user, method, path, body, None).await;
        assert!((200..300).contains(&status), "{path}: {status} {value}");
        value["data"].clone()
    }
    async fn terminal(&self, user: &User, base: &str, id: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let value = self
                    .ok(
                        user,
                        Method::GET,
                        &format!("{base}/executions/{id}"),
                        Value::Null,
                    )
                    .await;
                if matches!(
                    value["status"].as_str(),
                    Some("Completed" | "Failed" | "Cancelled")
                ) {
                    return value;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn workspace(&self, owner: &User) -> (String, String) {
        let orgs = self
            .ok(owner, Method::GET, "/api/v1/organizations", Value::Null)
            .await;
        let org = orgs[0]["id"].as_str().unwrap().to_owned();
        let ws = self
            .ok(
                owner,
                Method::POST,
                &format!("/api/v1/organizations/{org}/workspaces"),
                json!({"name":"Query workspace"}),
            )
            .await;
        let base = format!(
            "/api/v1/organizations/{org}/workspaces/{}",
            ws["id"].as_str().unwrap()
        );
        (org, base)
    }
    async fn member(&self, owner: &User, user: &User, org: &str, base: &str, role: &str) {
        self.ok(
            owner,
            Method::POST,
            &format!("/api/v1/organizations/{org}/members"),
            json!({"user_id":user.id,"role":"Member"}),
        )
        .await;
        self.ok(
            owner,
            Method::POST,
            &format!("{base}/members"),
            json!({"user_id":user.id,"role":role}),
        )
        .await;
    }
    async fn connection(&self, owner: &User, base: &str, engine: &str) -> Value {
        self.ok(owner,Method::POST,&format!("{base}/connections"),json!({"profile":{"id":Uuid::new_v4(),"revision":0,"name":engine,"engine":engine,"host":"127.0.0.1","port":if engine=="Postgres" {55432}else{53306},"username":"studio","database":"studio_test","ssl":"disable","connect_timeout_ms":5000,"query_timeout_ms":10000,"history_enabled":true,"restore_sql":true},"password":"studio_test"})).await
    }
    async fn execute(
        &self,
        user: &User,
        base: &str,
        session: &str,
        sql: &str,
        key: &str,
    ) -> String {
        let (status, value) = self
            .call(
                user,
                Method::POST,
                &format!("{base}/sessions/{session}/executions"),
                json!({"sql":sql}),
                Some(key),
            )
            .await;
        assert!((200..300).contains(&status), "execution {status}: {value}");
        value["data"]["execution_id"].as_str().unwrap().into()
    }
    async fn socket(
        &self,
        user: &User,
        base: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let mut request = format!(
            "ws://{}{base}/stream",
            self.origin.trim_start_matches("http://")
        )
        .into_client_request()
        .unwrap();
        request
            .headers_mut()
            .insert("Origin", self.origin.parse().unwrap());
        request
            .headers_mut()
            .insert("Cookie", user.cookie.parse().unwrap());
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "studio.v1".parse().unwrap());
        let (mut socket, _) = connect_async(request).await.unwrap();
        socket
            .send(Message::Text(
                json!({"type":"hello","version":1,"csrf":user.csrf,"tab_token":user.tab})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert_eq!(frame(&mut socket).await["type"], "ready");
        socket
    }
}
async fn frame(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Text(text) = socket.next().await.unwrap().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL/MySQL fixtures"]
async fn http_ws_engines_idempotency_privacy_and_tenant_guards() {
    let f = Fixture::open().await;
    let owner = f.login("owner").await;
    let developer = f.login("developer").await;
    let viewer = f.login("viewer").await;
    let stranger = f.login("stranger").await;
    let (org, base) = f.workspace(&owner).await;
    f.member(&owner, &developer, &org, &base, "Developer").await;
    f.member(&owner, &viewer, &org, &base, "Viewer").await;
    assert_eq!(
        f.call(
            &stranger,
            Method::GET,
            &format!("{base}/connections"),
            Value::Null,
            None
        )
        .await
        .0,
        403
    );
    let bad_origin = Client::new()
        .post(format!("{}{base}/connections/test", f.origin))
        .header("Cookie", &owner.cookie)
        .header("Origin", "https://evil.invalid")
        .header("x-csrf-token", &owner.csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_origin.status(), 422); // Body validation is safe; a valid body is checked below.
    let mut executions = Vec::new();
    for engine in ["Postgres", "MySql"] {
        let profile = f.connection(&owner, &base, engine).await;
        let conn = profile["id"].as_str().unwrap();
        let bad_csrf = Client::new()
            .post(format!("{}{base}/connections/{conn}/sessions", f.origin))
            .header("Cookie", &owner.cookie)
            .header("Origin", &f.origin)
            .send()
            .await
            .unwrap();
        assert_eq!(bad_csrf.status(), 403);
        let deny_origin = Client::new()
            .post(format!("{}{base}/connections/{conn}/sessions", f.origin))
            .header("Cookie", &owner.cookie)
            .header("Origin", "https://evil.invalid")
            .header("x-csrf-token", &owner.csrf)
            .send()
            .await
            .unwrap();
        assert_eq!(deny_origin.status(), 403);
        assert_eq!(
            f.call(
                &viewer,
                Method::POST,
                &format!("{base}/connections/{conn}/sessions"),
                Value::Null,
                None
            )
            .await
            .0,
            403
        );
        f.ok(
            &viewer,
            Method::POST,
            &format!("{base}/connections/{conn}/metadata"),
            json!("Databases"),
        )
        .await;
        let session = f
            .ok(
                &developer,
                Method::POST,
                &format!("{base}/connections/{conn}/sessions"),
                Value::Null,
            )
            .await;
        let sid = session["id"].as_str().unwrap();
        let other_tab = User {
            cookie: developer.cookie.clone(),
            csrf: developer.csrf.clone(),
            id: developer.id.clone(),
            tab: "b".repeat(43),
        };
        assert_eq!(
            f.call(
                &other_tab,
                Method::GET,
                &format!("{base}/sessions/{sid}"),
                Value::Null,
                None
            )
            .await
            .0,
            404
        );

        let sql = if engine == "Postgres" {
            "SELECT CAST(9007199254740993 AS BIGINT) AS precise, CAST(123456789012345.123456 AS DECIMAL(30,6)) AS amount, NULL AS empty_value"
        } else {
            "SELECT CAST(9007199254740993 AS SIGNED) AS precise, CAST(123456789012345.123456 AS DECIMAL(30,6)) AS amount, NULL AS empty_value"
        };
        let key = Uuid::new_v4().to_string();
        let execution = f.execute(&developer, &base, sid, sql, &key).await;
        let info = f.terminal(&developer, &base, &execution).await;
        assert_eq!(
            info["status"], "Completed",
            "engine={engine}, error={}, results={}",
            info["error"], info["results"]
        );
        assert_eq!(info["persistence_error"], Value::Null);
        assert_eq!(
            f.execute(&developer, &base, sid, sql, &key).await,
            execution
        );
        assert_eq!(
            f.call(
                &developer,
                Method::POST,
                &format!("{base}/sessions/{sid}/executions"),
                json!({"sql":"SELECT 2"}),
                Some(&key)
            )
            .await
            .0,
            409
        );
        assert_eq!(
            f.call(
                &owner,
                Method::GET,
                &format!("{base}/executions/{execution}"),
                Value::Null,
                None
            )
            .await
            .0,
            404
        );
        assert_eq!(
            f.call(
                &viewer,
                Method::GET,
                &format!("{base}/executions/{execution}"),
                Value::Null,
                None
            )
            .await
            .0,
            403
        );
        assert_eq!(
            f.call(
                &other_tab,
                Method::GET,
                &format!("{base}/executions/{execution}"),
                Value::Null,
                None
            )
            .await
            .0,
            404
        );
        let mut socket = f.socket(&developer, &base).await;
        socket.send(Message::Text(json!({"type":"page","request_id":"p1","execution_id":execution,"result_id":info["results"][0]["id"],"offset":0,"limit":256}).to_string().into())).await.unwrap();
        let page = frame(&mut socket).await;
        assert_eq!(page["type"], "page");
        assert_eq!(page["rows"][0][0]["value"], "9007199254740993");
        assert_eq!(page["rows"][0][1]["value"], "123456789012345.123456");
        assert_eq!(page["rows"][0][2]["kind"], "null");
        socket
            .send(Message::Text(
                json!({"type":"ack","sequence":page["sequence"]})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
        let history = f
            .ok(
                &developer,
                Method::GET,
                &format!("{base}/connections/{conn}/documents?kind=history"),
                Value::Null,
            )
            .await;
        assert_eq!(history.as_array().unwrap().len(), 1);
        let hidden = f
            .ok(
                &owner,
                Method::GET,
                &format!("{base}/connections/{conn}/documents?kind=history"),
                Value::Null,
            )
            .await;
        assert!(hidden.as_array().unwrap().is_empty());
        assert_eq!(
            f.call(
                &owner,
                Method::GET,
                &format!(
                    "{base}/connections/{conn}/documents/{}?kind=history",
                    history[0]["id"].as_str().unwrap()
                ),
                Value::Null,
                None
            )
            .await
            .0,
            404
        );
        let doc = Uuid::new_v4().to_string();
        let document_path = format!("{base}/connections/{conn}/documents");
        f.ok(&developer,Method::POST,&document_path,json!({"id":doc,"kind":"saved","name":"Shared","content":{"sql":"SELECT 1"},"revision":0})).await;
        assert_eq!(
            f.ok(
                &owner,
                Method::GET,
                &format!("{document_path}/{doc}?kind=saved"),
                Value::Null
            )
            .await["content"]["sql"],
            "SELECT 1"
        );
        assert_eq!(
            f.call(
                &viewer,
                Method::GET,
                &format!("{document_path}/{doc}?kind=saved"),
                Value::Null,
                None
            )
            .await
            .0,
            403
        );
        assert_eq!(f.call(&developer,Method::POST,&document_path,json!({"id":doc,"kind":"saved","name":"Stale","content":{"sql":"SELECT 2"},"revision":0}),None).await.0,409);
        let mut changed = profile.clone();
        changed["history_enabled"] = false.into();
        changed["restore_sql"] = false.into();
        f.ok(
            &owner,
            Method::PUT,
            &format!("{base}/connections/{conn}"),
            json!({"profile":changed,"password":null}),
        )
        .await;
        assert!(
            f.ok(
                &developer,
                Method::GET,
                &format!("{document_path}?kind=history"),
                Value::Null
            )
            .await
            .as_array()
            .unwrap()
            .is_empty()
        );
        let cancelled = f
            .execute(
                &developer,
                &base,
                sid,
                if engine == "Postgres" {
                    "SELECT pg_sleep(5); SELECT 999"
                } else {
                    "SELECT SLEEP(5); SELECT 999"
                },
                &Uuid::new_v4().to_string(),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        f.ok(
            &developer,
            Method::POST,
            &format!("{base}/executions/{cancelled}/cancel"),
            Value::Null,
        )
        .await;
        let terminal = f.terminal(&developer, &base, &cancelled).await;
        assert_eq!(terminal["status"], "Cancelled");
        assert!(terminal["results"].as_array().unwrap().len() <= 1);
        let failed = f
            .execute(
                &developer,
                &base,
                sid,
                "SELECT * FROM phase3_missing_relation; SELECT 999",
                &Uuid::new_v4().to_string(),
            )
            .await;
        assert_eq!(
            f.terminal(&developer, &base, &failed).await["status"],
            "Failed"
        );
        executions.push(execution);
        f.ok(
            &developer,
            Method::DELETE,
            &format!("{base}/sessions/{sid}?force=true"),
            Value::Null,
        )
        .await;
    }
    // Composite scope lookups and foreign keys reject swapped tenant/workspace identifiers.
    let other_org = Uuid::new_v4();
    let other_ws = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1,'Other tenant')")
        .bind(other_org)
        .execute(&f.app.store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces(organization_id,id,name) VALUES($1,$2,'Other workspace')")
        .bind(other_org)
        .bind(other_ws)
        .execute(&f.app.store.pool)
        .await
        .unwrap();
    let swapped = base.replace(&org, &other_org.to_string());
    assert_eq!(
        f.call(
            &owner,
            Method::GET,
            &format!("{swapped}/connections"),
            Value::Null,
            None
        )
        .await
        .0,
        403
    );
    let invalid_fk = sqlx::query("INSERT INTO workspace_members VALUES($1,$2,$3,'Developer')")
        .bind(other_org)
        .bind(other_ws)
        .bind(Uuid::parse_str(&owner.id).unwrap())
        .execute(&f.app.store.pool)
        .await;
    assert!(invalid_fk.is_err());
    let events: Vec<(String, String)> = sqlx::query_as("SELECT action,outcome FROM audit_events")
        .fetch_all(&f.app.store.pool)
        .await
        .unwrap();
    assert!(!events.is_empty());
    for (action, outcome) in events {
        assert!(!action.contains("SELECT"));
        assert!(!outcome.contains("studio_test"));
    }
    f.ok(&developer, Method::POST, "/api/v1/auth/logout", Value::Null)
        .await;
    assert_eq!(
        f.call(
            &developer,
            Method::GET,
            "/api/v1/auth/me",
            Value::Null,
            None
        )
        .await
        .0,
        401
    );
    let count = f.app.runtime.snapshot(
        studio_api::store::Scope {
            organization: Uuid::parse_str(&org).unwrap(),
            workspace: Uuid::parse_str(base.rsplit('/').next().unwrap()).unwrap(),
        },
        &studio_api::auth::Principal {
            user: Uuid::parse_str(&developer.id).unwrap(),
            login: studio_api::auth::hash(developer.cookie.split('=').nth(1).unwrap()),
            csrf_hash: studio_api::auth::hash(&developer.csrf),
            tab: Some(studio_api::auth::hash(&developer.tab)),
        },
        Uuid::parse_str(&executions[0]).unwrap(),
    );
    assert!(count.is_err());
    f.close().await;
}

#[tokio::test]
#[ignore = "requires disposable application PostgreSQL"]
async fn expiry_schema_guard_and_second_instance_do_not_mutate_running_records() {
    let f = Fixture::open().await;
    let owner = f.login("owner").await;
    sqlx::query("UPDATE sessions SET last_activity=now()-interval '31 minutes'")
        .execute(&f.app.store.pool)
        .await
        .unwrap();
    assert_eq!(
        f.call(&owner, Method::GET, "/api/v1/auth/me", Value::Null, None)
            .await
            .0,
        401
    );
    assert_eq!(
        f.call(
            &owner,
            Method::POST,
            "/api/v1/auth/activity",
            Value::Null,
            None
        )
        .await
        .0,
        401
    );
    assert_eq!(
        Store::migrate(&f.app.store.pool).await.unwrap_err().code,
        "INSTANCE_IN_USE"
    );
    let options = PgConnectOptions::from_str(&f.app.config.database_url)
        .unwrap()
        .disable_statement_logging();
    // A distinct maintenance test schema verifies downgrade protection without bypassing the live lock.
    let maintenance = format!("maintenance_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {maintenance}"))
        .execute(&f.admin)
        .await
        .unwrap();
    let options = options.options([("search_path", maintenance.as_str())]);
    let maintenance_pool = PgPool::connect_with(options).await.unwrap();
    Store::migrate(&maintenance_pool).await.unwrap();
    sqlx::query("UPDATE studio_schema SET version=3")
        .execute(&maintenance_pool)
        .await
        .unwrap();
    assert_eq!(
        Store::migrate(&maintenance_pool).await.unwrap_err().code,
        "SCHEMA_INCOMPATIBLE"
    );
    sqlx::query("UPDATE studio_schema SET version=2")
        .execute(&maintenance_pool)
        .await
        .unwrap();
    maintenance_pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {maintenance} CASCADE"))
        .execute(&f.admin)
        .await
        .unwrap();
    let key = Arc::new(KeyRing::from_key("test", [17; 32]));
    let store = Arc::new(
        Store::connect(&f.app.config.database_url, key)
            .await
            .unwrap(),
    );
    assert!(studio_api::runtime::Runtime::open(store.clone(), f.app.config.clone()).is_err());
    store.pool.close().await;
    f.close().await;
}

#[tokio::test]
#[ignore = "requires disposable application/customer PostgreSQL and MySQL"]
async fn productivity_search_favorites_and_atomic_http_mutations() {
    let f = Fixture::open().await;
    let owner = f.login("owner").await;
    let peer = f.login("productivity-peer").await;
    let viewer = f.login("productivity-viewer").await;
    let (org, base) = f.workspace(&owner).await;
    f.member(&owner, &peer, &org, &base, "Developer").await;
    f.member(&owner, &viewer, &org, &base, "Viewer").await;
    for engine in ["Postgres", "MySql"] {
        let profile = f.connection(&owner, &base, engine).await;
        let conn = profile["id"].as_str().unwrap();
        let cp = format!("{base}/connections/{conn}");
        let doc = Uuid::new_v4();
        f.ok(&owner, Method::POST, &format!("{cp}/documents"), json!({"id":doc,"kind":"saved","name":"Private favorite","content":{"sql":"SELECT productivity_needle"},"revision":0})).await;
        f.ok(
            &owner,
            Method::PUT,
            &format!("{cp}/documents/{doc}/favorite"),
            json!({"favorite":true}),
        )
        .await;
        let search = json!({"kind":"saved","query":"PRODUCTIVITY_NEEDLE","favorites_only":true});
        let results = f
            .ok(
                &owner,
                Method::POST,
                &format!("{cp}/documents/search"),
                search.clone(),
            )
            .await;
        assert_eq!(results["items"].as_array().unwrap().len(), 1);
        assert!(results["items"][0].get("content").is_none());
        assert_eq!(
            f.ok(
                &peer,
                Method::POST,
                &format!("{cp}/documents/search"),
                search.clone()
            )
            .await["items"],
            json!([])
        );
        assert_eq!(
            f.call(
                &viewer,
                Method::POST,
                &format!("{cp}/documents/search"),
                search,
                None
            )
            .await
            .0,
            403
        );
        let session = f
            .ok(&owner, Method::POST, &format!("{cp}/sessions"), Value::Null)
            .await;
        let sid = session["id"].as_str().unwrap();
        let name = format!("http_edit_{}", Uuid::new_v4().simple());
        let sql = format!(
            "CREATE TABLE {name} (id BIGINT PRIMARY KEY,label VARCHAR(100) DEFAULT 'default'); INSERT INTO {name} (id,label) VALUES(9007199254740993,'original')"
        );
        let execution = f
            .execute(&owner, &base, sid, &sql, &Uuid::new_v4().to_string())
            .await;
        assert_eq!(
            f.terminal(&owner, &base, &execution).await["status"],
            "Completed"
        );
        let tab = Uuid::new_v4();
        let object = json!({"engine":engine,"database":"studio_test","schema":if engine=="Postgres" {Some("public")} else {None},"name":name,"kind":"Table"});
        let snapshot = f
            .ok(
                &owner,
                Method::POST,
                &format!("{cp}/edits/snapshot"),
                json!({"object":object,"tab_id":tab}),
            )
            .await;
        assert_eq!(snapshot["rows"][0][0]["value"], "9007199254740993");
        let batch = json!({"tab_id":tab,"token":snapshot["token"],"changes":[{"type":"update","row":0,"values":{"label":{"type":"value","value":"changed"}}}]});
        assert_eq!(
            f.call(
                &peer,
                Method::POST,
                &format!("{cp}/edits/preview"),
                batch.clone(),
                None
            )
            .await
            .0,
            422
        );
        f.ok(
            &owner,
            Method::POST,
            &format!("{cp}/edits/preview"),
            batch.clone(),
        )
        .await;
        let key = Uuid::new_v4().to_string();
        let (status, response) = f
            .call(
                &owner,
                Method::POST,
                &format!("{cp}/mutations"),
                batch.clone(),
                Some(&key),
            )
            .await;
        assert_eq!(status, 202);
        let id = response["data"]["id"].as_str().unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let value = f
                    .ok(
                        &owner,
                        Method::GET,
                        &format!("{cp}/mutations/{id}"),
                        Value::Null,
                    )
                    .await;
                if value["state"] != "Applying" {
                    break value;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(outcome["state"], "Applied");
        assert_eq!(
            f.call(
                &owner,
                Method::POST,
                &format!("{cp}/mutations"),
                batch,
                Some(&key)
            )
            .await
            .1["data"],
            outcome
        );
        assert_eq!(
            f.call(
                &peer,
                Method::GET,
                &format!("{cp}/mutations/{id}"),
                Value::Null,
                None
            )
            .await
            .0,
            404
        );
        let receipt = f
            .ok(
                &owner,
                Method::POST,
                &format!("{cp}/edits/readback"),
                json!({"tab_id":tab,"token":snapshot["token"]}),
            )
            .await;
        assert_eq!(receipt[0][1]["value"], "changed");
        assert_eq!(
            f.call(
                &peer,
                Method::POST,
                &format!("{cp}/edits/readback"),
                json!({"tab_id":tab,"token":snapshot["token"]}),
                None
            )
            .await
            .0,
            422
        );
        f.ok(
            &owner,
            Method::DELETE,
            &format!("{cp}/edits/{tab}"),
            Value::Null,
        )
        .await;
        let snapshot = f
            .ok(
                &owner,
                Method::POST,
                &format!("{cp}/edits/snapshot"),
                json!({"object":object,"tab_id":tab}),
            )
            .await;
        assert_eq!(snapshot["rows"][0][1]["value"], "changed");
        let execution = f
            .execute(
                &owner,
                &base,
                sid,
                &format!("DROP TABLE {name}"),
                &Uuid::new_v4().to_string(),
            )
            .await;
        assert_eq!(
            f.terminal(&owner, &base, &execution).await["status"],
            "Completed"
        );
        f.ok(
            &owner,
            Method::DELETE,
            &format!("{base}/sessions/{sid}?force=true"),
            Value::Null,
        )
        .await;
    }
    f.close().await;
}
