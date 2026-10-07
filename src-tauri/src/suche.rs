//! Den Studio-Server im Netz finden.
//!
//! Zuerst per mDNS: Der Server kündigt sich als `_studio-edge._tcp` an, mit Mandant und Name
//! (Dienst `ansage` im Edge-Compose). Antwortet so keiner, fragt die App das eigene /24-Netz
//! direkt ab – 64 Adressen gleichzeitig, je höchstens 400 ms. Ändert sich die Adresse des
//! Servers (DHCP), findet ihn der Agent so wieder: am selben Mandanten.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use futures::stream::{self, StreamExt};
use serde::Serialize;

use crate::edge;

const DIENST: &str = "_studio-edge._tcp.local.";

#[derive(Clone, Debug, Serialize)]
pub struct EdgeFund {
    pub adresse: String,
    pub name: String,
    pub tenant_id: String,
    pub version: String,
    pub quelle: &'static str,
}

fn mdns(dauer: Duration) -> Vec<EdgeFund> {
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else { return vec![] };
    let Ok(empfang) = daemon.browse(DIENST) else {
        let _ = daemon.shutdown();
        return vec![];
    };
    let ende = std::time::Instant::now() + dauer;
    let mut funde = Vec::new();
    while let Some(rest) = ende.checked_duration_since(std::time::Instant::now()) {
        match empfang.recv_timeout(rest) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                let port = info.get_port();
                let tenant = info.get_property_val_str("tenant").unwrap_or_default().to_string();
                let name = info.get_property_val_str("name").unwrap_or_default().to_string();
                let version = info.get_property_val_str("version").unwrap_or_default().to_string();
                for a in info.get_addresses().iter() {
                    let ip = a.to_string();
                    if ip.contains(':') {
                        continue; // IPv6 lassen wir aus – der Server spricht im LAN IPv4
                    }
                    let adresse = if port == 80 { format!("http://{ip}") } else { format!("http://{ip}:{port}") };
                    funde.push(EdgeFund { adresse, name: name.clone(), tenant_id: tenant.clone(), version: version.clone(), quelle: "mdns" });
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    funde
}

fn eigene_netze() -> Vec<Ipv4Addr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v4) if v4.is_private() => Some(v4),
            _ => None,
        })
        .collect()
}

async fn netz_abfragen() -> Vec<EdgeFund> {
    let mut adressen = Vec::new();
    for eigen in eigene_netze() {
        let [a, b, c, _] = eigen.octets();
        for d in 1..=254u8 {
            let ip = Ipv4Addr::new(a, b, c, d);
            if ip != eigen {
                adressen.push(format!("http://{ip}"));
            }
        }
    }
    stream::iter(adressen)
        .map(|basis| async move {
            edge::kopf(&basis, Duration::from_millis(400)).await.ok().map(|k| EdgeFund {
                adresse: basis, name: k.name, tenant_id: k.tenant_id, version: k.version, quelle: "netz",
            })
        })
        .buffer_unordered(64)
        .filter_map(|x| async move { x })
        .collect()
        .await
}

/// Alle Studio-Server, die antworten – je Adresse einmal.
pub async fn suchen() -> Vec<EdgeFund> {
    let mut funde = tauri::async_runtime::spawn_blocking(|| mdns(Duration::from_millis(2500))).await.unwrap_or_default();
    // mDNS-Antworten bestätigen: nur wer auch über HTTP antwortet, kommt in die Liste
    let mut bestaetigt = Vec::new();
    for f in funde.drain(..) {
        if edge::kopf(&f.adresse, Duration::from_secs(2)).await.is_ok() {
            bestaetigt.push(f);
        }
    }
    if bestaetigt.is_empty() {
        bestaetigt = netz_abfragen().await;
    }
    let mut je: BTreeMap<String, EdgeFund> = BTreeMap::new();
    for f in bestaetigt {
        je.entry(f.adresse.clone()).or_insert(f);
    }
    je.into_values().collect()
}

/// Den Server dieses Mandanten wiederfinden, wenn er unter der alten Adresse schweigt.
pub async fn wiederfinden(tenant_id: &str) -> Option<String> {
    suchen().await.into_iter().find(|f| f.tenant_id == tenant_id).map(|f| f.adresse)
}
