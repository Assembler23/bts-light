//! mDNS-Bekanntgabe des LAN-Tablet-Servers.
//!
//! Im LAN-Modus meldet sich bts-light im Hallennetz unter dem **festen
//! Namen** `bts-light.local`. Tablets und Court-Monitore erreichen den
//! Turnier-PC darüber, **ohne seine IP-Adresse zu kennen** – es ist
//! keine feste IP nötig, weder im Router noch am Laptop. Der Laptop darf
//! per DHCP jede beliebige Adresse bekommen.

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::tablet::server::TABLET_PORT;

/// Fester mDNS-Hostname des Turnier-PCs. Monitore/Tablets nutzen
/// `http://bts-light.local:8088/…`. Muss mit dem im Frontend verwendeten
/// Namen übereinstimmen.
pub const MDNS_HOST: &str = "bts-light.local";

/// Startet die mDNS-Bekanntgabe von `bts-light.local` → lokale IP.
///
/// Der zurückgegebene [`ServiceDaemon`] muss am Leben bleiben – wird er
/// verworfen, endet die Bekanntgabe. Ein Fehler ist unkritisch: die
/// direkte IP-Adresse des Servers funktioniert weiterhin.
pub fn advertise() -> Result<ServiceDaemon, String> {
    let ip = local_ip_address::local_ip().map_err(|e| e.to_string())?;
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let service = ServiceInfo::new(
        "_bts-light._tcp.local.",
        "bts-light",
        "bts-light.local.",
        ip,
        TABLET_PORT,
        &[] as &[(&str, &str)],
    )
    .map_err(|e| e.to_string())?;
    daemon.register(service).map_err(|e| e.to_string())?;
    tracing::info!("mDNS: {MDNS_HOST} → {ip}:{TABLET_PORT} bekanntgegeben");
    Ok(daemon)
}
