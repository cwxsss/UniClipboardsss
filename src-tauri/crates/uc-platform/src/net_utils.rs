use local_ip_address::list_afinet_netifas;
use std::net::{IpAddr, Ipv4Addr};
use tracing::{debug, warn};

/// Detect the best physical LAN IPv4 address for libp2p to listen on.
///
/// 检测最佳的物理局域网 IPv4 地址，供 libp2p 监听使用。
///
/// # Filtering rules / 过滤规则
/// - Exclude loopback (127.*)
/// - Exclude link-local (169.254.*)
/// - Exclude tunnel interfaces (utun, tun, tap)
/// - Exclude Clash TUN addresses (198.18.0.0/15)
/// - Only keep private IPv4 addresses (10.*, 172.16-31.*, 192.168.*)
pub fn get_physical_lan_ip() -> Option<Ipv4Addr> {
    let interfaces = match list_afinet_netifas() {
        Ok(ifaces) => ifaces,
        Err(e) => {
            warn!(error = %e, "failed to enumerate network interfaces");
            return None;
        }
    };

    for (iface_name, ip) in interfaces {
        if is_loopback_interface(&iface_name) {
            debug!(interface = %iface_name, "skip loopback interface for p2p listen");
            continue;
        }

        if let IpAddr::V4(v4) = ip {
            if v4.is_loopback() || v4.is_link_local() {
                debug!(interface = %iface_name, ip = %v4, "skip non-routable interface address");
                continue;
            }

            if is_tunnel_interface(&iface_name) {
                debug!(interface = %iface_name, ip = %v4, "skip tunnel interface for p2p listen");
                continue;
            }

            if is_clash_tun_address(v4) {
                debug!(interface = %iface_name, ip = %v4, "skip clash tun address for p2p listen");
                continue;
            }

            if is_private_ipv4(v4) {
                debug!(ip = %v4, interface = %iface_name, "detected physical LAN IP");
                return Some(v4);
            }

            debug!(interface = %iface_name, ip = %v4, "skip non-private ipv4 for p2p listen");
        }
    }

    warn!("no suitable physical LAN IP found");
    None
}

fn is_loopback_interface(name: &str) -> bool {
    name == "lo" || name.starts_with("lo")
}

fn is_tunnel_interface(name: &str) -> bool {
    name.contains("utun") || name.contains("tun") || name.contains("tap")
}

fn is_clash_tun_address(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 198 && octets[1] >= 18
}

fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    match octets[0] {
        10 => true,
        172 => (16..=31).contains(&octets[1]),
        192 => octets[1] == 168,
        _ => false,
    }
}
