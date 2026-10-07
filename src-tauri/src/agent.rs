//! Der Geräte-Agent: Herzschlag zum Studio-Server und Aufträge an die Geräte am Platz.
//!
//! Zwei Schleifen nebeneinander:
//! * **Herzschlag** alle 30 s – meldet Version, PC und USB-Geräte, bekommt die Einstellungen
//!   des Platzes zurück. Bleibt der Server dreimal stumm, zeigt das Fenster „keine
//!   Verbindung“, und der Agent sucht den Server seines Mandanten im Netz (neue Adresse?).
//! * **Aufträge** per Long-Poll – jeder Auftrag läuft für sich, damit ein Abbruch einen
//!   wartenden Finger erreicht.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::edge::{Agentinfo, Auftrag, Edge, EdgeFehler, Ergebnis};
use crate::geraete::{self, secugen};
use crate::{fenster, konfig, krypto, suche, Zustand};

const HERZSCHLAG: Duration = Duration::from_secs(30);
const STUMM_BIS_OFFLINE: u32 = 3;

pub fn agentinfo() -> Agentinfo {
    let (hersteller, modell) = geraete::rechner();
    Agentinfo {
        agent_version: env!("CARGO_PKG_VERSION").into(),
        system: system(),
        rechner: hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default(),
        hersteller,
        modell,
        seriennummer: machine_uid::get().ok(),
    }
}

fn system() -> String {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;
        if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion") {
            let name: String = k.get_value("ProductName").unwrap_or_else(|_| "Windows".into());
            let ver: String = k.get_value("DisplayVersion").unwrap_or_default();
            let build: String = k.get_value("CurrentBuild").unwrap_or_default();
            // Windows 11 meldet sich in ProductName weiter als „Windows 10“ – am Build erkennbar
            let name = if build.parse::<u32>().unwrap_or(0) >= 22000 { name.replace("Windows 10", "Windows 11") } else { name };
            return format!("{name} {ver}").trim().to_string();
        }
    }
    std::env::consts::OS.to_string()
}

/// Beide Schleifen (neu) starten – nach dem Koppeln und beim Programmstart.
pub fn starten(app: &AppHandle) {
    let z = app.state::<Zustand>();
    let mut laeufe = z.agent.lock().unwrap();
    for h in laeufe.drain(..) {
        h.abort();
    }
    let a = app.clone();
    laeufe.push(tauri::async_runtime::spawn(async move { herzschlag(a).await }));
    let a = app.clone();
    laeufe.push(tauri::async_runtime::spawn(async move { auftraege(a).await }));
}

pub fn anhalten(app: &AppHandle) {
    for h in app.state::<Zustand>().agent.lock().unwrap().drain(..) {
        h.abort();
    }
}

/// Der Server kennt das Token nicht mehr: alles vergessen, zurück zur Einrichtung.
pub fn entkoppelt(app: &AppHandle, meldung: &str) {
    let z = app.state::<Zustand>();
    konfig::geheimnisse_loeschen();
    konfig::Konfig::loeschen(&z.ordner);
    *z.konfig.lock().unwrap() = None;
    *z.edge_ursprung.lock().unwrap() = None;
    *z.hinweis.lock().unwrap() = Some(meldung.to_string());
    z.auf_oberflaeche.store(false, Ordering::Relaxed);
    anhalten(app);
    fenster::modus(app, "bedient");
    fenster::zeige_lokal(app, "index.html");
}

async fn herzschlag(app: AppHandle) {
    let mut stumm = 0u32;
    loop {
        let z = app.state::<Zustand>();
        let Some(edge) = z.edge() else { return };
        let info = agentinfo();
        let usb = tauri::async_runtime::spawn_blocking(geraete::suchen).await.unwrap_or_default();
        match edge.herzschlag(&info, &usb).await {
            Ok(platz) => {
                stumm = 0;
                let geaendert = {
                    let mut k = z.konfig.lock().unwrap();
                    match k.as_mut() {
                        Some(k) if k.art != platz.art || k.platz_name != platz.name || k.ausrichtung != platz.ausrichtung
                            || k.platz_nr != platz.nr => {
                            k.art = platz.art.clone();
                            k.platz_name = platz.name.clone();
                            k.platz_nr = platz.nr.clone();
                            k.ausrichtung = platz.ausrichtung.clone();
                            let _ = k.speichern(&z.ordner);
                            true
                        }
                        _ => false,
                    }
                };
                *z.zuletzt.lock().unwrap() = Some(platz.clone());
                let offline = z.offline.swap(false, Ordering::Relaxed);
                if offline || geaendert {
                    // Wieder da oder die Art hat sich geändert: Oberfläche neu öffnen
                    if z.auf_oberflaeche.load(Ordering::Relaxed) || offline {
                        if let Err(e) = fenster::zur_oberflaeche(&app).await {
                            log::warn!("Oberfläche öffnen: {e:#}");
                        }
                    }
                }
            }
            Err(EdgeFehler::NichtGekoppelt) => {
                entkoppelt(&app, "Der Platz wurde am Studio-Server entkoppelt. Bitte neu koppeln.");
                return;
            }
            Err(EdgeFehler::Netz(m)) => {
                stumm += 1;
                log::warn!("Herzschlag: {m}");
                if stumm >= STUMM_BIS_OFFLINE && !z.offline.swap(true, Ordering::Relaxed) && !z.in_wartung() {
                    z.auf_oberflaeche.store(false, Ordering::Relaxed);
                    fenster::zeige_lokal(&app, "offline.html");
                }
                if stumm % STUMM_BIS_OFFLINE == 0 {
                    neue_adresse_suchen(&app).await;
                }
            }
            Err(e) => log::warn!("Herzschlag: {e}"),
        }
        tokio::time::sleep(HERZSCHLAG).await;
    }
}

/// Schweigt der Server, kann er eine neue Adresse haben (DHCP): am Mandanten wiedererkennen.
async fn neue_adresse_suchen(app: &AppHandle) {
    let z = app.state::<Zustand>();
    let (tenant, alt) = match z.konfig.lock().unwrap().as_ref() {
        Some(k) => (k.tenant_id.clone(), k.edge.clone()),
        None => return,
    };
    if let Some(neu) = suche::wiederfinden(&tenant).await {
        if neu != alt {
            log::info!("Studio-Server unter neuer Adresse: {neu}");
            if let Some(k) = z.konfig.lock().unwrap().as_mut() {
                k.edge = neu;
                let _ = k.speichern(&z.ordner);
            }
        }
    }
}

async fn auftraege(app: AppHandle) {
    loop {
        let Some(edge) = app.state::<Zustand>().edge() else { return };
        match edge.auftraege().await {
            Ok(liste) => {
                for a in liste {
                    let (app, edge) = (app.clone(), edge.clone());
                    tauri::async_runtime::spawn(async move { bearbeiten(app, edge, a).await });
                }
            }
            Err(EdgeFehler::NichtGekoppelt) => return,
            Err(e) => {
                log::debug!("Aufträge: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

fn fehler(meldung: impl Into<String>) -> Ergebnis {
    Ergebnis { ok: false, daten: json!({}), meldung: Some(meldung.into()), code: None }
}

fn gut(daten: Value) -> Ergebnis {
    Ergebnis { ok: true, daten, meldung: None, code: None }
}

async fn bearbeiten(app: AppHandle, edge: Edge, a: Auftrag) {
    let z = app.state::<Zustand>();
    let ergebnis = match a.art.as_str() {
        "abbrechen" => {
            if let Some(id) = a.daten["auftrag"].as_str() {
                if let Some(f) = z.abbrueche.lock().unwrap().get(id) {
                    f.store(true, Ordering::Relaxed);
                }
            }
            return;
        }
        "usb_suchen" => {
            let usb = tauri::async_runtime::spawn_blocking(geraete::suchen).await.unwrap_or_default();
            gut(json!({ "geraete": usb }))
        }
        "geraet_testen" => match a.daten["typ"].as_str() {
            Some("finger") => match tauri::async_runtime::spawn_blocking(secugen::testen).await {
                Ok(Ok(m)) => gut(json!({ "meldung": m })),
                Ok(Err(e)) => fehler(format!("{e:#}")),
                Err(e) => fehler(e.to_string()),
            },
            _ => fehler("Dieses Gerät wird am Platz noch nicht unterstützt"),
        },
        "finger_aufnehmen" => finger(&app, &a).await,
        andere => fehler(format!("Unbekannter Auftrag „{andere}“ – App aktualisieren?")),
    };
    if let Err(e) = edge.ergebnis(&a.id, &ergebnis).await {
        log::warn!("Ergebnis zu {} nicht angekommen: {e}", a.art);
    }
}

async fn finger(app: &AppHandle, a: &Auftrag) -> Ergebnis {
    let z = app.state::<Zustand>();
    let Some(schluessel) = konfig::schluessel_lesen() else {
        return fehler("Bildschlüssel fehlt – Platz neu koppeln");
    };
    let zeit_ms = a.daten["zeit_ms"].as_u64().unwrap_or(10_000).clamp(1_000, 60_000);
    let qualitaet = a.daten["qualitaet"].as_u64().unwrap_or(50).min(100) as u32;
    let abbruch = Arc::new(AtomicBool::new(false));
    z.abbrueche.lock().unwrap().insert(a.id.clone(), abbruch.clone());
    let klasse = geraete::suchen().into_iter().find(|g| g.typ == "finger").and_then(|g| g.klasse);
    let auftrag = a.id.clone();
    let erg = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Value> {
        let aufnahme = secugen::aufnehmen(zeit_ms, qualitaet, &abbruch)?;
        let png = secugen::png(&aufnahme)?;
        Ok(json!({
            "bild": krypto::verschluesseln(&schluessel, &auftrag, &png)?,
            "breite": aufnahme.breite, "hoehe": aufnahme.hoehe, "klasse": klasse,
            "qualitaet_platz": aufnahme.qualitaet, "seriennummer": aufnahme.seriennummer,
        }))
    })
    .await;
    z.abbrueche.lock().unwrap().remove(&a.id);
    match erg {
        Ok(Ok(daten)) => gut(daten),
        Ok(Err(e)) => {
            let m = format!("{e:#}");
            let code = if m.contains("Kein Finger") { Some(54) } else if m.contains("Abgebrochen") { Some(-1) } else { None };
            Ergebnis { ok: false, daten: json!({}), meldung: Some(m), code }
        }
        Err(e) => fehler(e.to_string()),
    }
}

/// Bildschlüssel als base64 prüfen (für die Einrichtung).
pub fn schluessel_gueltig(s: &str) -> bool {
    B64.decode(s).map(|k| k.len() == 32).unwrap_or(false)
}
