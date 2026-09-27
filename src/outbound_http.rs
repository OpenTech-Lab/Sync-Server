use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::{redirect, Client, Url};

use crate::config::Config;
use crate::errors::AppError;

const DEFAULT_HTTPS_PORT: u16 = 443;
const MAX_REDIRECTS: usize = 0;

fn is_restricted_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        || octets[0] >= 240
}

fn is_restricted_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_restricted_ipv4(mapped);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00 // unique-local fc00::/7
        || (segments[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (segments[0] == 0x2001 && segments[1] == 0x0db8) // documentation
}

fn is_restricted_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_restricted_ipv4(ip),
        IpAddr::V6(ip) => is_restricted_ipv6(ip),
    }
}

fn development_loopback_allowed(cfg: &Config, host: &str) -> bool {
    !cfg.app_env.eq_ignore_ascii_case("production")
        && (host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false))
}

pub fn parse_federation_url(raw: &str, cfg: &Config) -> Result<Url, AppError> {
    let url = Url::parse(raw.trim())
        .map_err(|e| AppError::BadRequest(format!("Invalid federation URL: {e}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("Federation URL host missing".into()))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::BadRequest(
            "Federation URLs must not contain credentials".into(),
        ));
    }
    let scheme_allowed = url.scheme() == "https"
        || (url.scheme() == "http" && development_loopback_allowed(cfg, host));
    if !scheme_allowed {
        return Err(AppError::BadRequest(
            "Federation URLs must use HTTPS".into(),
        ));
    }
    Ok(url)
}

/// Builds a no-redirect client pinned to DNS results that were checked before
/// connection. Pinning closes the validation/use gap that enables DNS rebinding.
pub async fn restricted_client(url: &Url, cfg: &Config) -> Result<Client, AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("Federation URL host missing".into()))?;
    let allow_loopback = development_loopback_allowed(cfg, host);
    let port = url.port_or_known_default().unwrap_or(DEFAULT_HTTPS_PORT);

    let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| AppError::BadRequest("Federation host could not be resolved".into()))?
        .collect();
    if resolved.is_empty()
        || resolved
            .iter()
            .any(|address| is_restricted_ip(address.ip()) && !allow_loopback)
    {
        return Err(AppError::Forbidden);
    }

    Client::builder()
        .redirect(if MAX_REDIRECTS == 0 {
            redirect::Policy::none()
        } else {
            redirect::Policy::limited(MAX_REDIRECTS)
        })
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .resolve_to_addrs(host, &resolved)
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("HTTP client setup failed: {e}")))
}

pub async fn client_for(raw: &str, cfg: &Config) -> Result<(Url, Client), AppError> {
    let url = parse_federation_url(raw, cfg)?;
    let client = restricted_client(&url, cfg).await?;
    Ok((url, client))
}

#[cfg(test)]
mod tests {
    use super::{is_restricted_ip, is_restricted_ipv4};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn blocks_internal_and_metadata_addresses() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(
                is_restricted_ip(value.parse::<IpAddr>().unwrap()),
                "{value}"
            );
        }
    }

    #[test]
    fn permits_public_addresses() {
        assert!(!is_restricted_ipv4(Ipv4Addr::new(1, 1, 1, 1)));
        assert!(!is_restricted_ipv4(Ipv4Addr::new(8, 8, 8, 8)));
    }
}
