//! LAN network discovery for mobile pairing.
//!
//! Returns the IPv4 address of the primary LAN interface so the mobile
//! companion can reach the desktop backend without going through the
//! public internet. Loopback and link-local addresses are skipped.

use serde::Serialize;
use std::net::IpAddr;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanEndpoint {
    /// IPv4 address picked for the LAN interface, e.g. "192.168.1.42".
    /// `None` when no usable interface is found (offline / link-down).
    pub ip: Option<String>,
    /// HTTP port the desktop backend (dccd-http) listens on. Defaults to 9876.
    pub port: u16,
    /// Pre-built `http://<ip>:<port>` URL, or `None` if `ip` is `None`.
    pub url: Option<String>,
}

/// Resolves the best LAN IPv4 to advertise to the mobile client.
///
/// Strategy:
/// 1. Ask the OS for the default-route source address (works on most
///    multi-homed setups by hitting `local-ip-address`).
/// 2. Reject loopback (`127.x.x.x`) and link-local (`169.254.x.x`) addresses.
/// 3. Prefer private RFC1918 ranges (`10/8`, `172.16/12`, `192.168/16`).
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

/// Combines `detect_lan_ip` with the configured backend port into a
/// serializable endpoint shape.
pub fn lan_endpoint(port: u16) -> LanEndpoint {
    let ip = detect_lan_ip();
    let url = ip.as_ref().map(|ip| format!("http://{ip}:{port}"));
    LanEndpoint { ip, port, url }
}
