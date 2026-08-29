//! One mDNS responder, so `familyhub.local` resolves on the home LAN
//! (PLAN v2 D3′, task T1.3).
//!
//! This is a **convenience, never a dependency**: the kiosk URL of record is
//! `http://<dhcp-reserved-ip>:8080/tv` (PURPLE_TEAM.md §P5.5 default 34) and
//! the TV's join QR encodes a raw-IP HTTPS URL, precisely because Fire OS
//! 7/8 will not resolve `.local` (RR-7). What this buys is a name the
//! parents' phones and any laptop can type.
//!
//! Two constraints from PURPLE_TEAM.md §P5.4's `mdns-sd` row shape the
//! module:
//!
//! * **Exactly one `ServiceDaemon` per process.** Several daemons
//!   intermittently hang on Windows (mdns-sd #478), so the daemon lives in a
//!   `OnceLock` and [`daemon`] is the only way to reach it.
//! * **No standalone hostname API** (#374): the A record for
//!   `familyhub.local.` is published as a side effect of registering a
//!   service whose `host_name` is that FQDN — **with the trailing dot**, or
//!   mdns-sd treats it as a relative name.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::OnceLock;

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::server::pki::host_ipv4_addresses;

/// The fully-qualified `.local` name, trailing dot included.
pub const MDNS_FQDN: &str = "familyhub.local.";

/// Service type advertised for the TV/HTTP origin.
pub const HTTP_SERVICE_TYPE: &str = "_http._tcp.local.";
/// Service type advertised for the phone/HTTPS origin.
pub const HTTPS_SERVICE_TYPE: &str = "_https._tcp.local.";

/// Instance name shown by any Bonjour/Avahi browser on the LAN.
const INSTANCE_NAME: &str = "Sheffield Family Hub";

static DAEMON: OnceLock<Result<ServiceDaemon, String>> = OnceLock::new();

/// The process's single [`ServiceDaemon`], started on first use.
///
/// Returns `Err` (rather than panicking) when the daemon cannot start —
/// typically a firewall blocking UDP 5353. A hub with no mDNS is a hub the
/// owner reaches by IP, which is the documented primary path anyway, so
/// this must never be fatal.
pub fn daemon() -> Result<&'static ServiceDaemon, &'static str> {
    DAEMON
        .get_or_init(|| ServiceDaemon::new().map_err(|err| err.to_string()))
        .as_ref()
        .map_err(String::as_str)
}

/// Register `familyhub.local.` together with the two service records.
///
/// Both services carry the same `host_name`, so the A record is published
/// once and answers for either. Returns the addresses actually advertised
/// so the caller can log them.
pub fn register(http_port: u16, https_port: u16) -> Result<Vec<Ipv4Addr>, String> {
    let addresses = host_ipv4_addresses();
    if addresses.is_empty() {
        return Err("no non-loopback IPv4 address to advertise".to_string());
    }

    let daemon = daemon()?;
    let ips: Vec<IpAddr> = addresses.iter().copied().map(IpAddr::V4).collect();

    for (service_type, port, path) in [
        (HTTP_SERVICE_TYPE, http_port, "/tv"),
        (HTTPS_SERVICE_TYPE, https_port, "/m"),
    ] {
        let info = ServiceInfo::new(
            service_type,
            INSTANCE_NAME,
            MDNS_FQDN,
            &ips[..],
            port,
            &[("path", path)][..],
        )
        .map_err(|err| format!("could not describe {service_type}: {err}"))?;

        daemon
            .register(info)
            .map_err(|err| format!("could not register {service_type}: {err}"))?;
    }

    tracing::info!(
        hostname = MDNS_FQDN,
        ?addresses,
        http_port,
        https_port,
        "advertising the hub over mDNS"
    );
    Ok(addresses)
}

/// Register, logging (never propagating) a failure. This is what the server
/// startup path calls: mDNS is a convenience and must not be able to stop
/// the hub from serving.
pub fn register_best_effort(http_port: u16, https_port: u16) {
    match register(http_port, https_port) {
        Ok(_) => {}
        Err(err) => tracing::warn!(
            %err,
            "mDNS advertisement unavailable; reach the hub by IP \
             (the kiosk URL of record is an IP anyway)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::pki::MDNS_HOSTNAME;

    #[test]
    fn the_advertised_hostname_is_an_fqdn_with_a_trailing_dot() {
        // mdns-sd #374: without the trailing dot the name is treated as
        // relative and no usable A record is published.
        assert!(
            MDNS_FQDN.ends_with('.'),
            "mDNS host names must be fully qualified"
        );
        assert_eq!(MDNS_FQDN, format!("{MDNS_HOSTNAME}."));
    }

    #[test]
    fn both_service_types_are_dns_sd_shaped() {
        for ty in [HTTP_SERVICE_TYPE, HTTPS_SERVICE_TYPE] {
            assert!(ty.ends_with("._tcp.local."), "{ty} is not a DNS-SD type");
        }
    }

    #[test]
    fn there_is_exactly_one_daemon_per_process() {
        // Two calls must hand back the same daemon; a second `ServiceDaemon`
        // is what hangs on Windows (mdns-sd #478).
        let Ok(first) = daemon() else {
            // No mDNS on this machine (firewall / no interface): the
            // single-daemon invariant is still what the code enforces, and
            // `register_best_effort` is what makes that non-fatal.
            return;
        };
        let second = daemon().expect("the second call returns the same result as the first");
        assert!(
            std::ptr::eq(first, second),
            "daemon() must hand back the one process-wide ServiceDaemon"
        );
    }
}
