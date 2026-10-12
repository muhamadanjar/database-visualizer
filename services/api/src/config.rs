use crate::{
    error::{ApiError, Result},
    keys::{KeyRing, SecretStore},
};
use ipnet::IpNet;
use query_runtime::Profile;
use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

#[derive(Clone, Deserialize)]
pub struct TargetRule {
    pub host: String,
    pub port: u16,
    pub network: IpNet,
}
#[derive(Clone)]
pub struct NetworkPolicy {
    pub rules: Vec<TargetRule>,
    pub protected: Vec<SocketAddr>,
    pub development: bool,
}
impl NetworkPolicy {
    pub fn validate(&self, profile: &Profile) -> Result<()> {
        if !self
            .rules
            .iter()
            .any(|rule| rule.host.eq_ignore_ascii_case(&profile.host) && rule.port == profile.port)
        {
            return Err(ApiError::denied());
        }
        if let Ok(ip) = profile.host.parse::<IpAddr>() {
            self.validate_addresses(profile, &[ip])?;
        }
        if !self.development && profile.ssl != "verify_full" {
            return Err(ApiError::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "TLS_VERIFICATION_REQUIRED",
                "Verified TLS is required",
            ));
        }
        let _ = profile.config(String::new())?;
        if profile.name.len() > 512
            || profile.username.len() > 512
            || profile.database.len() > 512
            || profile.host.len() > 253
        {
            return Err(ApiError::invalid());
        }
        Ok(())
    }
    /// Check every DNS answer before selecting a destination. Mixed answers fail closed.
    pub fn validate_addresses(&self, profile: &Profile, addresses: &[IpAddr]) -> Result<IpAddr> {
        if addresses.is_empty() {
            return Err(ApiError::unavailable());
        }
        for address in addresses {
            let ip = address.to_canonical();
            if is_metadata_address(ip)
                || self
                    .protected
                    .iter()
                    .any(|target| target.ip().to_canonical() == ip && target.port() == profile.port)
                || !self.rules.iter().any(|rule| {
                    rule.host.eq_ignore_ascii_case(&profile.host)
                        && rule.port == profile.port
                        && rule.network.contains(&ip)
                })
            {
                return Err(ApiError::denied());
            }
        }
        Ok(addresses[0].to_canonical())
    }
    pub async fn resolve(&self, profile: &Profile) -> Result<IpAddr> {
        self.validate(profile)?;
        if let Ok(ip) = profile.host.parse::<IpAddr>() {
            return self.validate_addresses(profile, &[ip]);
        }
        let addresses = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::lookup_host((profile.host.as_str(), profile.port)),
        )
        .await
        .map_err(|_| ApiError::unavailable())?
        .map_err(|_| ApiError::unavailable())?
        .map(|address| address.ip())
        .collect::<Vec<_>>();
        self.validate_addresses(profile, &addresses)
    }
}
fn is_metadata_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.octets() == [100, 100, 100, 200]
        }
        IpAddr::V6(ip) => {
            ip.is_unicast_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip
                    == "fd00:ec2::254"
                        .parse::<std::net::Ipv6Addr>()
                        .unwrap_or(std::net::Ipv6Addr::UNSPECIFIED)
        }
    }
}
#[derive(Clone)]
pub struct Config {
    pub origin: String,
    pub bind: SocketAddr,
    pub database_url: zeroize::Zeroizing<String>,
    pub keys: Arc<dyn SecretStore>,
    pub spool_directory: PathBuf,
    pub network: NetworkPolicy,
    pub bootstrap_owner: String,
    pub bootstrap_name: String,
    pub audit_days: i32,
    pub github: Option<OAuthClient>,
    pub google: Option<OAuthClient>,
}
#[derive(Clone)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: zeroize::Zeroizing<String>,
}
fn required(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| ApiError::unavailable())
}
impl Config {
    pub async fn load() -> Result<Self> {
        let origin = required("STUDIO_WEB_ORIGIN")?;
        let url = url::Url::parse(&origin).map_err(|_| ApiError::invalid())?;
        let development = std::env::var("STUDIO_DEVELOPMENT").as_deref() == Ok("1");
        if origin != url.origin().ascii_serialization()
            || !url.username().is_empty()
            || url.password().is_some()
            || !(url.scheme() == "https"
                || development
                    && url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
        {
            return Err(ApiError::invalid());
        }
        let database_url = zeroize::Zeroizing::new(required("STUDIO_APPLICATION_DATABASE_URL")?);
        let app = url::Url::parse(&database_url).map_err(|_| ApiError::invalid())?;
        let host = app.host_str().ok_or_else(ApiError::invalid)?;
        let port = app.port().unwrap_or(5432);
        if !development && !matches!(app.host(), Some(url::Host::Ipv4(_) | url::Host::Ipv6(_))) {
            return Err(ApiError::invalid());
        }
        let protected = tokio::net::lookup_host((host.trim_matches(['[', ']']), port))
            .await
            .map_err(|_| ApiError::unavailable())?
            .collect();
        let rules: Vec<TargetRule> = serde_json::from_slice(
            &std::fs::read(required("STUDIO_TARGET_POLICY_FILE")?)
                .map_err(|_| ApiError::unavailable())?,
        )
        .map_err(|_| ApiError::invalid())?;
        if rules.is_empty()
            || rules.len() > 256
            || rules.iter().any(|rule| {
                rule.host.is_empty()
                    || rule.host.len() > 253
                    || !rule
                        .host
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".-:".contains(&byte))
                    || rule.port == 0
            })
        {
            return Err(ApiError::invalid());
        }
        let provider = |name: &str| -> Result<Option<OAuthClient>> {
            match (
                std::env::var(format!("STUDIO_{name}_CLIENT_ID")),
                std::env::var(format!("STUDIO_{name}_CLIENT_SECRET_FILE")),
            ) {
                (Err(_), Err(_)) => Ok(None),
                (Ok(client_id), Ok(path)) => {
                    let metadata =
                        std::fs::symlink_metadata(&path).map_err(|_| ApiError::unavailable())?;
                    if !metadata.is_file() || metadata.len() > 65536 {
                        return Err(ApiError::invalid());
                    }
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if metadata.permissions().mode() & 0o077 != 0 {
                            return Err(ApiError::unavailable());
                        }
                    }
                    let secret = zeroize::Zeroizing::new(
                        std::fs::read_to_string(path).map_err(|_| ApiError::unavailable())?,
                    );
                    if client_id.is_empty() || secret.trim().is_empty() {
                        return Err(ApiError::invalid());
                    }
                    Ok(Some(OAuthClient {
                        client_id,
                        client_secret: zeroize::Zeroizing::new(secret.trim().to_owned()),
                    }))
                }
                _ => Err(ApiError::invalid()),
            }
        };
        let bootstrap_owner = required("STUDIO_BOOTSTRAP_OWNER")?;
        let (provider_name, subject) = bootstrap_owner
            .split_once(':')
            .ok_or_else(ApiError::invalid)?;
        if !matches!(provider_name, "github" | "google")
            && !(development && provider_name == "test")
            || subject.is_empty()
            || subject.len() > 255
        {
            return Err(ApiError::invalid());
        }
        let bootstrap_name = std::env::var("STUDIO_BOOTSTRAP_ORGANIZATION")
            .unwrap_or_else(|_| "Database Studio".into());
        if bootstrap_name.trim().is_empty() || bootstrap_name.len() > 512 {
            return Err(ApiError::invalid());
        }
        Ok(Self {
            origin,
            bind: std::env::var("STUDIO_API_BIND")
                .unwrap_or_else(|_| "127.0.0.1:8080".into())
                .parse()
                .map_err(|_| ApiError::invalid())?,
            database_url,
            keys: Arc::new(KeyRing::load(std::path::Path::new(&required(
                "STUDIO_KEY_FILE",
            )?))?),
            spool_directory: PathBuf::from(required("STUDIO_SPOOL_DIRECTORY")?),
            network: NetworkPolicy {
                rules,
                protected,
                development,
            },
            bootstrap_owner,
            bootstrap_name,
            audit_days: std::env::var("STUDIO_AUDIT_DAYS")
                .unwrap_or_else(|_| "90".into())
                .parse()
                .ok()
                .filter(|n| *n >= 1 && *n <= 3650)
                .ok_or_else(ApiError::invalid)?,
            github: provider("GITHUB")?,
            google: provider("GOOGLE")?,
        })
    }
}
