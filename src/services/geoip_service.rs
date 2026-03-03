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
    registered_country: Option<MmdbCountry>,
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
        let ips = resolve_host_ips(&host);
        if ips.is_empty() {
            return Self::default();
        }

        let Ok(reader) = Reader::open_readfile(mmdb_path) else {
            return Self::default();
        };

        for ip in ips {
            if is_local_ip(ip) {
                continue;
            }

            let Ok(record) = reader.lookup::<MmdbCountryRecord>(ip) else {
                continue;
            };

            let country = record.country.or(record.registered_country);
            let country_code = country
                .as_ref()
                .and_then(|c| c.iso_code.clone())
                .map(|v| v.to_uppercase());
            let country_name = country
                .as_ref()
                .and_then(|c| c.names.as_ref())
                .and_then(|names| names.get("en").cloned());

            if country_code.is_some() || country_name.is_some() {
                return Self {
                    country_code,
                    country_name,
                };
            }
        }

        Self::default()
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

fn resolve_host_ips(host: &str) -> Vec<IpAddr> {
    if host.is_empty() {
        return Vec::new();
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return vec![ip];
    }

    let mut ips = (host, 0)
        .to_socket_addrs()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|addr| match addr.ip() {
            IpAddr::V4(v4) => Some(IpAddr::V4(v4)),
            IpAddr::V6(v6) => Some(IpAddr::V6(v6)),
        })
        .collect::<Vec<_>>();
    // Prefer IPv4 first, then IPv6. Some free GeoIP DBs can be sparse for IPv6.
    ips.sort_by_key(|ip| if matches!(ip, IpAddr::V4(_)) { 0 } else { 1 });
    ips.dedup();
    ips
}
