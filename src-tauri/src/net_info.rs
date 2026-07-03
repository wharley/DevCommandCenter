//! LAN + Tailscale network discovery for mobile pairing.
//!
//! Builds the list of HTTP base URLs the mobile companion could use to reach
//! the desktop backend. Modeled after T3 Code's "advertised endpoints" pattern:
//! each entry carries an explicit reachability tier (loopback, lan,
//! private-network, public) so the UI can present them with sensible defaults.

use serde::Serialize;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;

/// How "far" an endpoint can be reached from. Mirrors T3 Code's contract so
/// the UI can sort / badge consistently.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reachability {
    Loopback,
    Lan,
    PrivateNetwork,
    Public,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointStatus {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvertisedEndpoint {
    /// Stable id, e.g. "lan:192.168.1.42" or "tailscale-magicdns:host.tail1234.ts.net".
    pub id: String,
    /// Human label, e.g. "LAN", "Tailscale IP", "Tailscale HTTPS".
    pub label: String,
    /// The provider that surfaced this endpoint.
    pub provider: &'static str,
    /// http(s) base URL the mobile client connects to.
    pub url: String,
    pub reachability: Reachability,
    pub status: EndpointStatus,
    /// One-line description shown beneath the option in the UI.
    pub description: String,
}

/// Backward-compatible single-LAN-URL shape kept while the older
/// `pair_get_lan_url` command exists. Prefer `discover_endpoints` for new UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanEndpoint {
    pub ip: Option<String>,
    pub port: u16,
    pub url: Option<String>,
}

/// Resolves the primary LAN IPv4. Rejects loopback / link-local / unspecified.
pub fn detect_lan_ip() -> Option<String> {
    for (_, ip) in local_ip_address::list_afinet_netifas().ok()? {
        let IpAddr::V4(v4) = ip else {
            continue;
        };
        if v4.is_loopback()
            || v4.is_link_local()
            || v4.is_unspecified()
            || !v4.is_private()
            || is_tailscale_ipv4(&v4.to_string())
        {
            continue;
        }
        return Some(v4.to_string());
    }
    None
}

pub fn lan_endpoint(port: u16) -> LanEndpoint {
    let ip = detect_lan_ip();
    let url = ip.as_ref().map(|ip| format!("http://{ip}:{port}"));
    LanEndpoint { ip, port, url }
}

// =============================================================================
// Tailscale
// =============================================================================

/// Subset of `tailscale status --json` we care about — only `Self.DNSName`
/// (MagicDNS) and `Self.TailscaleIPs`. The CLI returns much more; we ignore
/// the rest to stay forward-compatible.
#[derive(Debug, Clone)]
pub struct TailscaleStatus {
    pub magic_dns_name: Option<String>,
    pub tailnet_ipv4: Vec<String>,
}

const TAILSCALE_STATUS_TIMEOUT_MS: u64 = 1_500;

/// True for any IP in 100.64.0.0/10 — the CGNAT space Tailscale advertises
/// over. Same check T3 Code uses.
fn is_tailscale_ipv4(addr: &str) -> bool {
    let mut parts = addr.split('.');
    let a = parts.next().and_then(|p| p.parse::<u8>().ok());
    let b = parts.next().and_then(|p| p.parse::<u8>().ok());
    let c = parts.next().and_then(|p| p.parse::<u8>().ok());
    let d = parts.next().and_then(|p| p.parse::<u8>().ok());
    if parts.next().is_some() {
        return false;
    }
    matches!((a, b, c, d), (Some(100), Some(b), Some(_), Some(_)) if (64..=127).contains(&b))
}

fn detect_tailscale_ipv4_addrs() -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let Ok(ifaces) = local_ip_address::list_afinet_netifas() else {
        return out;
    };

    for (_, ip) in ifaces {
        let IpAddr::V4(v4) = ip else {
            continue;
        };
        let addr = v4.to_string();
        if is_tailscale_ipv4(&addr) && seen.insert(addr.clone()) {
            out.push(addr);
        }
    }

    out
}

fn tcp_endpoint_status(ip: &str, port: u16) -> EndpointStatus {
    let Ok(ip_addr) = ip.parse::<IpAddr>() else {
        return EndpointStatus::Unknown;
    };
    let addr = SocketAddr::from((ip_addr, port));
    match TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
        Ok(_) => EndpointStatus::Available,
        Err(_) => EndpointStatus::Unavailable,
    }
}

/// Runs `tailscale status --json` with a short timeout and parses the relevant
/// fields. Returns `None` when the binary is missing, the daemon is offline,
/// or the call times out — pairing always falls back to LAN-only in that case.
pub fn read_tailscale_status() -> Option<TailscaleStatus> {
    // Use a manual timeout via a worker thread because std::process::Command
    // has no built-in timeout knob and we do not want to drag in a runtime here.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = Command::new("tailscale")
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let _ = tx.send(result);
    });

    let output = rx
        .recv_timeout(Duration::from_millis(TAILSCALE_STATUS_TIMEOUT_MS))
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let self_obj = json.get("Self")?;

    let magic_dns_name = self_obj
        .get("DNSName")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().trim_end_matches('.').to_string())
        .filter(|s| !s.is_empty());

    let tailnet_ipv4 = self_obj
        .get("TailscaleIPs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|ip| is_tailscale_ipv4(ip))
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(TailscaleStatus {
        magic_dns_name,
        tailnet_ipv4,
    })
}

// =============================================================================
// Endpoint composition
// =============================================================================

/// Produces the full set of endpoints to advertise: loopback + LAN + Tailscale
/// (IPs and optional MagicDNS HTTPS). Ordered roughly by usefulness for mobile
/// pairing — Tailscale first when present (works from anywhere), then LAN,
/// then loopback as a fallback the user can still see.
pub fn discover_endpoints(port: u16) -> Vec<AdvertisedEndpoint> {
    let mut out: Vec<AdvertisedEndpoint> = Vec::new();
    let mut seen_tailscale_ips = std::collections::HashSet::new();

    // Tailscale IPs are network interfaces on this machine. Read them directly
    // so the HTTP endpoint remains available even when `tailscale status` is
    // blocked, slow, or prompts on macOS sandboxed Tailscale installs.
    for ip in detect_tailscale_ipv4_addrs() {
        seen_tailscale_ips.insert(ip.clone());
        out.push(AdvertisedEndpoint {
            id: format!("tailscale-ip:{ip}"),
            label: "Tailscale".to_string(),
            provider: "tailscale",
            url: format!("http://{ip}:{port}"),
            reachability: Reachability::PrivateNetwork,
            status: tcp_endpoint_status(&ip, port),
            description: "Funciona em qualquer rede (4G, hotel WiFi, etc) se o celular também estiver no Tailnet.".to_string(),
        });
    }

    // Tailscale CLI is still useful for MagicDNS and as a fallback source of
    // IPs, but the interface scan above is the authoritative path for HTTP.
    if let Some(ts) = read_tailscale_status() {
        for ip in &ts.tailnet_ipv4 {
            if !seen_tailscale_ips.insert(ip.clone()) {
                continue;
            }
            out.push(AdvertisedEndpoint {
                id: format!("tailscale-ip:{ip}"),
                label: "Tailscale".to_string(),
                provider: "tailscale",
                url: format!("http://{ip}:{port}"),
                reachability: Reachability::PrivateNetwork,
                status: tcp_endpoint_status(ip, port),
                description: "Funciona em qualquer rede (4G, hotel WiFi, etc) se o celular também estiver no Tailnet.".to_string(),
            });
        }
        if let Some(dns) = &ts.magic_dns_name {
            // Tailscale Serve uses HTTPS on 443 (default). We don't probe here
            // to keep the call fast; the UI can warn if it does not respond.
            out.push(AdvertisedEndpoint {
                id: format!("tailscale-magicdns:{dns}"),
                label: "Tailscale HTTPS".to_string(),
                provider: "tailscale",
                url: format!("https://{dns}"),
                reachability: Reachability::PrivateNetwork,
                status: EndpointStatus::Unknown,
                description:
                    "Requer `tailscale serve` configurado no desktop. Certificado válido, sem aviso no celular."
                        .to_string(),
            });
        }
    }

    // LAN.
    if let Some(ip) = detect_lan_ip() {
        let status = tcp_endpoint_status(&ip, port);
        out.push(AdvertisedEndpoint {
            id: format!("lan:{ip}"),
            label: "LAN".to_string(),
            provider: "core",
            url: format!("http://{ip}:{port}"),
            reachability: Reachability::Lan,
            status,
            description: "Celular precisa estar na mesma WiFi que o desktop.".to_string(),
        });
    }

    // Loopback — always present, useful for desktop-local testing or running
    // the mobile client inside a browser on the same machine.
    out.push(AdvertisedEndpoint {
        id: "loopback".to_string(),
        label: "Loopback".to_string(),
        provider: "core",
        url: format!("http://127.0.0.1:{port}"),
        reachability: Reachability::Loopback,
        status: EndpointStatus::Available,
        description: "Apenas para testes no próprio computador.".to_string(),
    });

    out
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_ipv4_accepts_cgnat_range() {
        assert!(is_tailscale_ipv4("100.64.0.1"));
        assert!(is_tailscale_ipv4("100.100.42.7"));
        assert!(is_tailscale_ipv4("100.127.255.254"));
    }

    #[test]
    fn tailscale_ipv4_rejects_outside_cgnat() {
        assert!(!is_tailscale_ipv4("100.63.255.255"));
        assert!(!is_tailscale_ipv4("100.128.0.0"));
        assert!(!is_tailscale_ipv4("192.168.1.1"));
        assert!(!is_tailscale_ipv4("10.0.0.1"));
        assert!(!is_tailscale_ipv4("not-an-ip"));
        assert!(!is_tailscale_ipv4("100.64.0"));
        assert!(!is_tailscale_ipv4("100.64.0.1.2"));
    }
}
