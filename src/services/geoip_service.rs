use maxminddb::Reader;
use serde::Deserialize;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct PlanetGeoInfo {
    pub country_code: Option<String>,
    pub country_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MmdbCountryRecord {
    country: Option<MmdbCountry>,
}

#[derive(Debug, Deserialize)]
struct MmdbCountry {
    iso_code: Option<String>,
    names: Option<std::collections::HashMap<String, String>>,
}

fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

impl PlanetGeoInfo {
    pub fn detect(instance_domain: &str, mmdb_path: &Path) -> Self {
        let host = normalize_host(instance_domain);
        let Some(ip) = resolve_host_ip(&host) else {
            return Self::default();
        };

        if is_local_ip(ip) {
            return Self::default();
        }

        let Ok(reader) = Reader::open_readfile(mmdb_path) else {
            return Self::default();
        };

        let Ok(record) = reader.lookup::<MmdbCountryRecord>(ip) else {
            return Self::default();
        };

        let country = record.country;
        let country_code = country
            .as_ref()
            .and_then(|c| c.iso_code.clone())
            .map(|v| v.to_uppercase());
        let country_name = country
            .as_ref()
            .and_then(|c| c.names.as_ref())
            .and_then(|names| names.get("en").cloned());

        Self {
            country_code,
            country_name,
        }
    }
}

fn normalize_host(instance_domain: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(instance_domain) {
        if let Some(host) = url.host_str() {
            return host.to_string();
        }
    }
    instance_domain
        .split(':')
        .next()
        .unwrap_or(instance_domain)
        .trim()
        .to_string()
}

fn resolve_host_ip(host: &str) -> Option<IpAddr> {
    if host.is_empty() {
        return None;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    (host, 0)
        .to_socket_addrs()
        .ok()?
        .find(|addr| matches!(addr.ip(), IpAddr::V4(_) | IpAddr::V6(_)))
        .map(|addr| addr.ip())
}
