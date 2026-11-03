//! Resolve the real client address behind the reverse proxies.
//!
//! In production every request reaches the API from the nginx container, so
//! the TCP peer says nothing about the client. The edge proxy overwrites
//! `X-Real-IP` with the address it accepted the connection from, which makes
//! that header the one trustworthy source. `X-Forwarded-For` is not: nginx only
//! appends to it, so its first entry is whatever the client sent.
//!
//! `X-Real-IP` is honoured only when the peer is itself a private address.
//! A server exposed directly to the internet therefore cannot be fooled by a
//! client that sends the header.

use std::net::IpAddr;

use actix_governor::{KeyExtractor, SimpleKeyExtractionError};
use actix_web::dev::ServiceRequest;
use actix_web::http::header::HeaderMap;

const REAL_IP_HEADER: &str = "X-Real-IP";

fn is_private_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.is_private() || v4.is_loopback();
            }
            // fc00::/7 is the IPv6 unique-local range.
            v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// The client address for a request, or `None` if it has no peer address.
pub fn resolve(peer: Option<IpAddr>, headers: &HeaderMap) -> Option<IpAddr> {
    let peer = peer?;
    if !is_private_peer(peer) {
        return Some(peer);
    }
    let forwarded = headers
        .get(REAL_IP_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok());
    Some(forwarded.unwrap_or(peer))
}

/// Rate-limit key: one bucket per client, instead of one per proxy.
#[derive(Clone, Copy)]
pub struct ClientIpKeyExtractor;

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(&self, req: &ServiceRequest) -> Result<Self::Key, Self::KeyExtractionError> {
        let ip =
            resolve(req.peer_addr().map(|addr| addr.ip()), req.headers()).ok_or_else(|| {
                SimpleKeyExtractionError::new("Could not determine the client IP address")
            })?;
        Ok(match ip {
            // One customer usually owns a whole /56, so limit per prefix.
            IpAddr::V6(v6) => {
                let mut octets = v6.octets();
                octets[7..16].fill(0);
                IpAddr::V6(octets.into())
            }
            v4 => v4,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::header::{HeaderName, HeaderValue};

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        map
    }

    fn ip(raw: &str) -> Option<IpAddr> {
        Some(raw.parse().unwrap())
    }

    #[test]
    fn uses_real_ip_when_the_peer_is_the_proxy() {
        let map = headers(&[("x-real-ip", "203.0.113.7")]);
        assert_eq!(resolve(ip("172.19.0.5"), &map), ip("203.0.113.7"));
    }

    #[test]
    fn ignores_client_supplied_forwarded_for() {
        let map = headers(&[
            ("x-forwarded-for", "1.2.3.4, 203.0.113.7"),
            ("x-real-ip", "203.0.113.7"),
        ]);
        assert_eq!(resolve(ip("172.19.0.5"), &map), ip("203.0.113.7"));
    }

    #[test]
    fn ignores_real_ip_from_a_public_peer() {
        let map = headers(&[("x-real-ip", "1.2.3.4")]);
        assert_eq!(resolve(ip("198.51.100.9"), &map), ip("198.51.100.9"));
    }

    #[test]
    fn falls_back_to_the_peer_for_a_missing_or_invalid_header() {
        assert_eq!(resolve(ip("10.0.0.2"), &HeaderMap::new()), ip("10.0.0.2"));
        let map = headers(&[("x-real-ip", "not-an-ip")]);
        assert_eq!(resolve(ip("10.0.0.2"), &map), ip("10.0.0.2"));
    }

    #[test]
    fn no_peer_means_no_address() {
        assert_eq!(resolve(None, &HeaderMap::new()), None);
    }

    #[test]
    fn treats_mapped_and_unique_local_ipv6_peers_as_private() {
        let map = headers(&[("x-real-ip", "203.0.113.7")]);
        assert_eq!(resolve(ip("::ffff:172.19.0.5"), &map), ip("203.0.113.7"));
        assert_eq!(resolve(ip("fd00::1"), &map), ip("203.0.113.7"));
        assert_eq!(resolve(ip("2001:db8::1"), &map), ip("2001:db8::1"));
    }
}
