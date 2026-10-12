use crate::{
    App,
    error::{ApiError, Result, data},
};
use axum::{
    Json,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Principal {
    pub user: Uuid,
    pub login: Vec<u8>,
    pub csrf_hash: Vec<u8>,
    pub tab: Option<Vec<u8>>,
}
pub fn random_token() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
fn cookie(headers: &HeaderMap, name: &str) -> Result<String> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        for part in value.to_str().map_err(|_| ApiError::auth())?.split(';') {
            if let Some((key, value)) = part.trim().split_once('=')
                && key == name
            {
                if found.is_some() || value.len() > 256 {
                    return Err(ApiError::auth());
                }
                found = Some(value.to_owned());
            }
        }
    }
    found.ok_or_else(ApiError::auth)
}
pub fn same_origin(app: &App, headers: &HeaderMap) -> Result<()> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(app.config.origin.as_str())
    {
        return Err(ApiError::denied());
    }
    Ok(())
}
pub fn csrf(app: &App, headers: &HeaderMap, principal: &Principal) -> Result<()> {
    same_origin(app, headers)?;
    let token = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::denied)?;
    if !bool::from(hash(token).as_slice().ct_eq(&principal.csrf_hash)) {
        return Err(ApiError::denied());
    }
    Ok(())
}
pub async fn authenticate(app: &App, headers: &HeaderMap) -> Result<Principal> {
    let token = cookie(headers, "studio_session")?;
    if token.len() != 43 {
        return Err(ApiError::auth());
    }
    principal_for_hash(app, hash(&token)).await
}
pub async fn principal_for_hash(app: &App, login: Vec<u8>) -> Result<Principal> {
    principal_for_hash_store(&app.store, login).await
}
pub async fn principal_for_hash_store(
    store: &crate::store::Store,
    login: Vec<u8>,
) -> Result<Principal> {
    let row=tokio::time::timeout(std::time::Duration::from_secs(3),sqlx::query("SELECT user_id,csrf_hash FROM sessions WHERE token_hash=$1 AND NOT revoked AND expires_at>now() AND last_activity>now()-interval '30 minutes'").bind(&login).fetch_optional(&store.pool)).await.map_err(|_|ApiError::unavailable())??.ok_or_else(ApiError::auth)?;
    Ok(Principal {
        user: row.try_get("user_id")?,
        csrf_hash: row.try_get("csrf_hash")?,
        tab: None,
        login,
    })
}
impl FromRequestParts<Arc<App>> for Principal {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &Arc<App>) -> Result<Self> {
        let mut principal = authenticate(state, &parts.headers).await?;
        principal.tab = parts
            .headers
            .get("x-studio-tab")
            .map(|value| {
                let token = value
                    .to_str()
                    .ok()
                    .filter(|value| value.len() == 43)
                    .ok_or_else(ApiError::invalid)?;
                Ok::<_, ApiError>(hash(token))
            })
            .transpose()?;
        state
            .limiter
            .check(format!("http:{}", principal.user), 1200)?;
        Ok(principal)
    }
}
pub async fn me(State(app): State<Arc<App>>, principal: Principal) -> Result<Json<Value>> {
    let row=sqlx::query("SELECT u.display_name,s.csrf_cipher FROM users u JOIN sessions s ON s.user_id=u.id WHERE u.id=$1 AND s.token_hash=$2").bind(principal.user).bind(&principal.login).fetch_one(&app.store.pool).await?;
    let encrypted: Vec<u8> = row.try_get("csrf_cipher")?;
    let bytes = app.store.keys.open(&principal.login, &encrypted)?;
    let token = std::str::from_utf8(&bytes).map_err(|_| ApiError::unavailable())?;
    Ok(data(
        json!({"user_id":principal.user,"display_name":row.try_get::<String,_>("display_name")?,"csrf_token":token}),
    ))
}
pub async fn activity(
    State(app): State<Arc<App>>,
    principal: Principal,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    csrf(&app, &headers, &principal)?;
    sqlx::query("UPDATE sessions SET last_activity=now() WHERE token_hash=$1 AND NOT revoked AND expires_at>now() AND last_activity>now()-interval '30 minutes'").bind(principal.login).execute(&app.store.pool).await?;
    Ok(data(Value::Null))
}
pub async fn logout(
    State(app): State<Arc<App>>,
    principal: Principal,
    headers: HeaderMap,
) -> Result<Response> {
    csrf(&app, &headers, &principal)?;
    sqlx::query("UPDATE sessions SET revoked=true WHERE token_hash=$1")
        .bind(&principal.login)
        .execute(&app.store.pool)
        .await?;
    app.runtime.revoke_login(&principal.login).await;
    let mut response = data(Value::Null).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        format!(
            "studio_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
            if app.config.network.development {
                ""
            } else {
                "; Secure"
            }
        )
        .parse()
        .map_err(|_| ApiError::unavailable())?,
    );
    Ok(response)
}
pub async fn providers(State(app): State<Arc<App>>) -> Json<Value> {
    let mut values = Vec::new();
    if app.config.github.is_some() {
        values.push("github");
    }
    if app.config.google.is_some() {
        values.push("google");
    }
    #[cfg(feature = "test-provider")]
    if app.config.network.development {
        values.push("test");
    }
    data(values)
}
fn provider<'a>(
    app: &'a App,
    name: &str,
) -> Result<(&'a crate::config::OAuthClient, &'static str, &'static str)> {
    match name {
        "github" => Ok((
            app.config.github.as_ref().ok_or_else(ApiError::missing)?,
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token",
        )),
        "google" => Ok((
            app.config.google.as_ref().ok_or_else(ApiError::missing)?,
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
        )),
        _ => Err(ApiError::missing()),
    }
}
pub async fn login(State(app): State<Arc<App>>, Path(name): Path<String>) -> Result<Response> {
    let (provider, url, _) = provider(&app, &name)?;
    let state = random_token();
    let browser = random_token();
    let verifier = random_token();
    let nonce = random_token();
    let encrypted = app.store.keys.seal(
        &hash(&state),
        &serde_json::to_vec(&json!({"verifier":verifier,"nonce":nonce}))
            .map_err(|_| ApiError::unavailable())?,
    )?;
    sqlx::query(
        "INSERT INTO oauth_states(state_hash,provider,browser_hash,verifier) VALUES($1,$2,$3,$4)",
    )
    .bind(hash(&state))
    .bind(&name)
    .bind(hash(&browser))
    .bind(encrypted)
    .execute(&app.store.pool)
    .await?;
    let mut url = url::Url::parse(url).map_err(|_| ApiError::unavailable())?;
    let callback = format!("{}/api/v1/auth/{name}/callback", app.config.origin);
    url.query_pairs_mut().extend_pairs([
        ("client_id", provider.client_id.as_str()),
        ("redirect_uri", callback.as_str()),
        ("state", state.as_str()),
        ("response_type", "code"),
        (
            "code_challenge",
            URL_SAFE_NO_PAD.encode(hash(&verifier)).as_str(),
        ),
        ("code_challenge_method", "S256"),
        (
            "scope",
            if name == "google" {
                "openid profile email"
            } else {
                "read:user"
            },
        ),
        ("nonce", nonce.as_str()),
    ]);
    let mut response = (StatusCode::FOUND, [(header::LOCATION, url.as_str())]).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        format!(
            "studio_oauth={browser}; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age=300{}",
            if app.config.network.development {
                ""
            } else {
                "; Secure"
            }
        )
        .parse()
        .map_err(|_| ApiError::unavailable())?,
    );
    Ok(response)
}
#[derive(Deserialize)]
pub struct Callback {
    code: String,
    state: String,
}
pub async fn limited_json(mut response: reqwest::Response) -> Result<Value> {
    if !response.status().is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "AUTH_PROVIDER_FAILED",
            "Authentication provider rejected the request",
        ));
    }
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ApiError::unavailable())?
    {
        if body.len() + chunk.len() > 1024 * 1024 {
            return Err(ApiError::unavailable());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| ApiError::unavailable())
}
#[derive(Deserialize)]
struct GoogleClaims {
    sub: String,
    aud: Value,
    azp: Option<String>,
    nonce: String,
    #[serde(default)]
    name: String,
}
pub async fn callback(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Query(input): Query<Callback>,
    headers: HeaderMap,
) -> Result<Response> {
    if input.state.len() != 43 || input.code.len() > 4096 {
        return Err(ApiError::auth());
    }
    let (provider, _, token_url) = provider(&app, &name)?;
    let browser = cookie(&headers, "studio_oauth")?;
    let row=sqlx::query("DELETE FROM oauth_states WHERE state_hash=$1 AND provider=$2 AND browser_hash=$3 AND expires_at>now() RETURNING verifier").bind(hash(&input.state)).bind(&name).bind(hash(&browser)).fetch_optional(&app.store.pool).await?.ok_or_else(ApiError::auth)?;
    let bytes = app
        .store
        .keys
        .open(&hash(&input.state), &row.try_get::<Vec<u8>, _>("verifier")?)?;
    let state: Value = serde_json::from_slice(&bytes).map_err(|_| ApiError::auth())?;
    let verifier = state["verifier"].as_str().ok_or_else(ApiError::auth)?;
    let callback = format!("{}/api/v1/auth/{name}/callback", app.config.origin);
    let mut tokens = limited_json(
        app.http
            .post(token_url)
            .header(header::ACCEPT, "application/json")
            .form(&[
                ("client_id", provider.client_id.as_str()),
                ("client_secret", provider.client_secret.as_str()),
                ("code", input.code.as_str()),
                ("redirect_uri", callback.as_str()),
                ("code_verifier", verifier),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|_| ApiError::unavailable())?,
    )
    .await?;
    let (subject, display_name) = if name == "github" {
        let access = Zeroizing::new(
            tokens
                .as_object_mut()
                .and_then(|v| v.remove("access_token"))
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(ApiError::auth)?,
        );
        let user = limited_json(
            app.http
                .get("https://api.github.com/user")
                .bearer_auth(access.as_str())
                .send()
                .await
                .map_err(|_| ApiError::unavailable())?,
        )
        .await?;
        (
            user["id"].as_u64().ok_or_else(ApiError::auth)?.to_string(),
            user["name"]
                .as_str()
                .or(user["login"].as_str())
                .unwrap_or("GitHub user")
                .to_owned(),
        )
    } else {
        let token = Zeroizing::new(
            tokens
                .as_object_mut()
                .and_then(|v| v.remove("id_token"))
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(ApiError::auth)?,
        );
        let header = jsonwebtoken::decode_header(&token).map_err(|_| ApiError::auth())?;
        if header.alg != jsonwebtoken::Algorithm::RS256 {
            return Err(ApiError::auth());
        }
        let keys = limited_json(
            app.http
                .get("https://www.googleapis.com/oauth2/v3/certs")
                .send()
                .await
                .map_err(|_| ApiError::unavailable())?,
        )
        .await?;
        let jwks: jsonwebtoken::jwk::JwkSet =
            serde_json::from_value(keys).map_err(|_| ApiError::auth())?;
        let jwk = jwks
            .find(header.kid.as_deref().ok_or_else(ApiError::auth)?)
            .ok_or_else(ApiError::auth)?;
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_audience(&[&provider.client_id]);
        validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);
        let claims = jsonwebtoken::decode::<GoogleClaims>(
            &token,
            &jsonwebtoken::DecodingKey::from_jwk(jwk).map_err(|_| ApiError::auth())?,
            &validation,
        )
        .map_err(|_| ApiError::auth())?
        .claims;
        if state["nonce"].as_str() != Some(claims.nonce.as_str())
            || claims.sub.is_empty()
            || claims.sub.len() > 255
            || claims
                .azp
                .as_deref()
                .is_some_and(|value| value != provider.client_id)
            || claims.aud.as_array().is_some_and(|values| values.len() > 1)
                && claims.azp.as_deref() != Some(provider.client_id.as_str())
        {
            return Err(ApiError::auth());
        }
        (claims.sub, claims.name)
    };
    let user = app
        .store
        .login_identity(
            &name,
            &subject,
            &display_name,
            &app.config.bootstrap_owner,
            &app.config.bootstrap_name,
        )
        .await?;
    let mut response = create_session(&app, user).await?;
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(
        header::LOCATION,
        "/".parse().map_err(|_| ApiError::unavailable())?,
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        format!(
            "studio_oauth=; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age=0{}",
            if app.config.network.development {
                ""
            } else {
                "; Secure"
            }
        )
        .parse()
        .map_err(|_| ApiError::unavailable())?,
    );
    Ok(response)
}
pub async fn create_session(app: &App, user: Uuid) -> Result<Response> {
    let token = random_token();
    let csrf = random_token();
    let login = hash(&token);
    let encrypted = app.store.keys.seal(&login, csrf.as_bytes())?;
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,csrf_hash,csrf_cipher) VALUES($1,$2,$3,$4)",
    )
    .bind(login)
    .bind(user)
    .bind(hash(&csrf))
    .bind(encrypted)
    .execute(&app.store.pool)
    .await?;
    let mut response = data(json!({"user_id":user,"csrf_token":csrf})).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        format!(
            "studio_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=43200{}",
            if app.config.network.development {
                ""
            } else {
                "; Secure"
            }
        )
        .parse()
        .map_err(|_| ApiError::unavailable())?,
    );
    Ok(response)
}
#[cfg(feature = "test-provider")]
pub async fn test_login(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response> {
    same_origin(&app, &headers)?;
    if !app.config.network.development {
        return Err(ApiError::missing());
    }
    let subject = body["subject"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .ok_or_else(ApiError::invalid)?;
    let user = app
        .store
        .login_identity(
            "test",
            subject,
            subject,
            &app.config.bootstrap_owner,
            &app.config.bootstrap_name,
        )
        .await?;
    create_session(&app, user).await
}
