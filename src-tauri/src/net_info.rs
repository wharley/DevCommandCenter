//! LAN + Tailscale network discovery for mobile pairing.
//!
//! Builds the list of HTTP base URLs the mobile companion could use to reach
//! the desktop backend. Modeled after T3 Code's "advertised endpoints" pattern:
//! each entry carries an explicit reachability tier (loopback, lan,
//! private-network, public) so the UI can present them with sensible defaults.

use serde::Serialize;
use std::net::IpAddr;
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
    let primary = local_ip_address::local_ip().ok()?;
    let v4 = match primary {
        IpAddr::V4(addr) => addr,
        IpAddr::V6(_) => return None,
    };
    if v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() {
        return None;
    }
    Some(v4.to_string())
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

    // Tailscale (zero or more endpoints).
    if let Some(ts) = read_tailscale_status() {
        for ip in &ts.tailnet_ipv4 {
            out.push(AdvertisedEndpoint {
                id: format!("tailscale-ip:{ip}"),
                label: "Tailscale".to_string(),
                provider: "tailscale",
                url: format!("http://{ip}:{port}"),
                reachability: Reachability::PrivateNetwork,
                status: EndpointStatus::Available,
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
        out.push(AdvertisedEndpoint {
            id: format!("lan:{ip}"),
            label: "LAN".to_string(),
            provider: "core",
            url: format!("http://{ip}:{port}"),
            reachability: Reachability::Lan,
            status: EndpointStatus::Available,
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
