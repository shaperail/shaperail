use std::net::{IpAddr, SocketAddr};

use actix_web::HttpRequest;
use shaperail_core::ProxyConfig;

const MAX_FORWARDED_HOPS: usize = 32;

/// Resolves a request's canonical client IP without trusting caller-supplied
/// forwarding headers by default.
#[derive(Debug, Clone, Default)]
pub struct ClientIpResolver {
    trusted_proxies: Vec<ipnet::IpNet>,
}

impl ClientIpResolver {
    /// Builds a resolver from project configuration.
    pub fn new(config: Option<&ProxyConfig>) -> Self {
        Self {
            trusted_proxies: config
                .map(|proxy| proxy.trusted_proxies.clone())
                .unwrap_or_default(),
        }
    }

    /// Returns the canonical client IP for an HTTP request.
    ///
    /// `X-Forwarded-For` is considered only when the immediate socket peer is
    /// trusted. The chain is evaluated from right to left, discarding trusted
    /// proxy hops until the nearest untrusted address is found.
    pub fn resolve(&self, req: &HttpRequest) -> Option<IpAddr> {
        let peer = normalize_ip(req.peer_addr()?.ip());
        if !self.is_trusted(peer) {
            return Some(peer);
        }

        let mut hops = Vec::new();
        for value in req.headers().get_all("x-forwarded-for") {
            let Ok(value) = value.to_str() else {
                return Some(peer);
            };
            for hop in value.split(',') {
                if hops.len() == MAX_FORWARDED_HOPS {
                    return Some(peer);
                }
                hops.push(hop.trim());
            }
        }

        let mut leftmost_trusted = None;
        for hop in hops.into_iter().rev() {
            let Some(ip) = parse_forwarded_ip(hop) else {
                return Some(peer);
            };
            leftmost_trusted = Some(ip);
            if !self.is_trusted(ip) {
                return Some(ip);
            }
        }

        leftmost_trusted.or(Some(peer))
    }

    /// Returns whether `ip` belongs to a configured trusted proxy network.
    pub fn is_trusted(&self, ip: IpAddr) -> bool {
        let ip = normalize_ip(ip);
        self.trusted_proxies
            .iter()
            .any(|network| network.contains(&ip))
    }
}

fn parse_forwarded_ip(value: &str) -> Option<IpAddr> {
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value);
    value
        .parse::<IpAddr>()
        .ok()
        .or_else(|| value.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
        .map(normalize_ip)
}

fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ipv6) => ipv6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ipv6)),
        ipv4 => ipv4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn resolver(networks: &[&str]) -> ClientIpResolver {
        ClientIpResolver {
            trusted_proxies: networks
                .iter()
                .map(|network| network.parse().unwrap())
                .collect(),
        }
    }

    fn request(peer: Option<&str>, forwarded_for: Option<&str>) -> HttpRequest {
        let mut request = TestRequest::default();
        if let Some(peer) = peer {
            request = request.peer_addr(peer.parse().unwrap());
        }
        if let Some(forwarded_for) = forwarded_for {
            request = request.insert_header(("x-forwarded-for", forwarded_for));
        }
        request.to_http_request()
    }

    #[test]
    fn direct_request_uses_socket_peer() {
        let request = request(Some("198.51.100.10:4242"), None);
        assert_eq!(
            ClientIpResolver::default().resolve(&request),
            Some("198.51.100.10".parse().unwrap())
        );
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_header() {
        let request = request(Some("198.51.100.10:4242"), Some("203.0.113.7, 192.0.2.3"));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("198.51.100.10".parse().unwrap())
        );
    }

    #[test]
    fn trusted_proxy_uses_nearest_untrusted_hop() {
        let request = request(
            Some("10.0.0.5:4242"),
            Some("198.51.100.8, 192.0.2.7, 10.0.0.4"),
        );
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("192.0.2.7".parse().unwrap())
        );
    }

    #[test]
    fn attacker_supplied_left_hops_do_not_override_nearest_client() {
        let request = request(
            Some("10.0.0.5:4242"),
            Some("garbage, 198.51.100.8, 203.0.113.7"),
        );
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("203.0.113.7".parse().unwrap())
        );
    }

    #[test]
    fn malformed_nearest_hop_falls_back_to_socket_peer() {
        let request = request(Some("10.0.0.5:4242"), Some("198.51.100.8, not-an-ip"));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("10.0.0.5".parse().unwrap())
        );
    }

    #[test]
    fn trusted_proxy_without_header_uses_socket_peer() {
        let request = request(Some("10.0.0.5:4242"), None);
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("10.0.0.5".parse().unwrap())
        );
    }

    #[test]
    fn noncanonical_forwarding_headers_are_ignored() {
        let request = TestRequest::default()
            .peer_addr("10.0.0.5:4242".parse().unwrap())
            .insert_header(("forwarded", "for=198.51.100.8"))
            .insert_header(("x-real-ip", "198.51.100.8"))
            .to_http_request();
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("10.0.0.5".parse().unwrap())
        );
    }

    #[test]
    fn multiple_forwarded_header_lines_form_one_chain() {
        let request = TestRequest::default()
            .peer_addr("10.0.0.5:4242".parse().unwrap())
            .append_header(("x-forwarded-for", "198.51.100.8"))
            .append_header(("x-forwarded-for", "10.0.0.4"))
            .to_http_request();
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("198.51.100.8".parse().unwrap())
        );
    }

    #[test]
    fn all_trusted_hops_use_leftmost_address() {
        let request = request(Some("10.0.0.5:4242"), Some("10.0.0.2, 10.0.0.3"));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("10.0.0.2".parse().unwrap())
        );
    }

    #[test]
    fn ipv4_and_ipv6_socket_forms_are_normalized() {
        let ipv4 = request(Some("10.0.0.5:4242"), Some("198.51.100.8:8080"));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&ipv4),
            Some("198.51.100.8".parse().unwrap())
        );

        let ipv6 = request(Some("[2001:db8:1::5]:4242"), Some("[2001:db8:2::8]:8080"));
        assert_eq!(
            resolver(&["2001:db8:1::/48"]).resolve(&ipv6),
            Some("2001:db8:2::8".parse().unwrap())
        );
    }

    #[test]
    fn ipv4_mapped_ipv6_peer_matches_ipv4_network() {
        let request = request(Some("[::ffff:10.0.0.5]:4242"), Some("::ffff:198.51.100.8"));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("198.51.100.8".parse().unwrap())
        );
    }

    #[test]
    fn quoted_addresses_are_accepted() {
        let request = request(Some("10.0.0.5:4242"), Some("\"198.51.100.8\""));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("198.51.100.8".parse().unwrap())
        );
    }

    #[test]
    fn missing_socket_peer_does_not_trust_forwarded_header() {
        let request = request(None, Some("198.51.100.8"));
        assert_eq!(resolver(&["10.0.0.0/8"]).resolve(&request), None);
    }

    #[test]
    fn oversized_chain_falls_back_to_socket_peer() {
        let forwarded_for = std::iter::repeat_n("10.0.0.2", MAX_FORWARDED_HOPS + 1)
            .collect::<Vec<_>>()
            .join(",");
        let request = request(Some("10.0.0.5:4242"), Some(&forwarded_for));
        assert_eq!(
            resolver(&["10.0.0.0/8"]).resolve(&request),
            Some("10.0.0.5".parse().unwrap())
        );
    }
}
