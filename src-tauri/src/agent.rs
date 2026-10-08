//! Der Geräte-Agent: Herzschlag zum Studio-Server und Aufträge an die Geräte am Platz.
//!
//! Zwei Schleifen nebeneinander:
//! * **Herzschlag** alle 30 s – meldet Version, PC und USB-Geräte, bekommt die Einstellungen
//!   des Platzes zurück. Bleibt der Server dreimal stumm, zeigt das Fenster „keine
//!   Verbindung“, und der Agent sucht den Server seines Mandanten im Netz (neue Adresse?).
//! * **Aufträge** per Long-Poll – jeder Auftrag läuft für sich, damit ein Abbruch einen
//!   wartenden Finger erreicht.
//! * **Karten** vom Magnetkartenleser – ein Thread hält den COM-Anschluss offen, jede Karte geht
//!   verschlüsselt an den Server (`ich/ereignis`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::edge::{Agentinfo, Auftrag, Edge, EdgeFehler, Ergebnis};
use crate::geraete::{self, drucker, magnetkarte, secugen, unterschrift};
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
        windows_drucker: Some(drucker::liste()).filter(|l| !l.is_empty()),
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
    let a = app.clone();
    laeufe.push(tauri::async_runtime::spawn(async move { karten(a).await }));
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
                let zoom_neu = {
                    let mut k = z.konfig.lock().unwrap();
                    match k.as_mut() {
                        Some(k) if k.zoom != platz.zoom => {
                            k.zoom = platz.zoom;
                            let _ = k.speichern(&z.ordner);
                            true
                        }
                        _ => false,
                    }
                };
                if zoom_neu {
                    fenster::zoom_anwenden(&app);
                }
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
            Some("magnetkarte") => {
                let s = magnetkarte::stand();
                match s.anschluss {
                    Some(a) => gut(json!({ "meldung": format!(
                        "Magnetkartenleser an {a} bereit · {} Karten gelesen – zum Test eine Karte durchziehen", s.karten) })),
                    None => fehler(magnetkarte::zustand().1.unwrap_or_else(|| "Kein Magnetkartenleser verbunden".into())),
                }
            }
            _ => fehler("Dieses Gerät wird am Platz noch nicht unterstützt"),
        },
        "finger_aufnehmen" => finger(&app, &a).await,
        "drucken" => drucken(&a).await,
        "signotec" => pad_vorgang(&app, &edge, &a).await,
        "signotec_befehl" => {
            let vorgang = a.daten["vorgang"].as_str().unwrap_or_default();
            let cmd = a.daten["cmd"].as_str().unwrap_or_default().to_string();
            match z.pad_vorgaenge.lock().unwrap().get(vorgang) {
                Some(tx) => {
                    let _ = tx.send(unterschrift::Eingang::Befehl(cmd));
                    gut(json!({}))
                }
                None => fehler("Kein laufender Vorgang am Pad"),
            }
        }
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

/// Ein Vorgang am Unterschriftenpad: läuft im eigenen Thread; Zwischenstände (Stiftpunkte …) gehen
/// gebündelt als Meldung „signotec“ an den Server, das Ende als Ergebnis dieses Auftrags.
async fn pad_vorgang(app: &AppHandle, edge: &Edge, a: &Auftrag) -> Ergebnis {
    let z = app.state::<Zustand>();
    let vorgang = a.daten["vorgang"].as_str().unwrap_or(&a.id).to_string();
    let (tx, rx) = std::sync::mpsc::channel::<unterschrift::Eingang>();
    z.pad_vorgaenge.lock().unwrap().insert(vorgang.clone(), tx.clone());
    let (mtx, mrx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let melder = tauri::async_runtime::spawn(pad_meldungen(edge.clone(), vorgang.clone(), mrx));
    let befehl = a.daten.clone();
    let ende = tauri::async_runtime::spawn_blocking(move || {
        unterschrift::vorgang(&befehl, &|ev| { let _ = mtx.send(ev); }, rx, tx)
    })
    .await;
    // Erst alle Zwischenstände hinaus, dann das Ende – die Reihenfolge zählt am Server
    let _ = melder.await;
    z.pad_vorgaenge.lock().unwrap().remove(&vorgang);
    match ende {
        Ok(ereignisse) => gut(json!({ "ereignisse": ereignisse })),
        Err(e) => fehler(e.to_string()),
    }
}

/// Stiftpunkte sammeln (bis 150 ms bzw. 60 Stück) und als eine Meldung schicken; alles andere sofort.
async fn pad_meldungen(edge: Edge, vorgang: String, mut rx: tokio::sync::mpsc::UnboundedReceiver<Value>) {
    while let Some(erstes) = rx.recv().await {
        let mut stapel = vec![erstes];
        if stapel[0]["ev"] == "punkt" {
            let frist = tokio::time::sleep(Duration::from_millis(150));
            tokio::pin!(frist);
            loop {
                tokio::select! {
                    _ = &mut frist => break,
                    weiter = rx.recv() => match weiter {
                        Some(v) => {
                            let schluss = v["ev"] != "punkt";
                            stapel.push(v);
                            if schluss || stapel.len() >= 60 {
                                break;
                            }
                        }
                        None => break,
                    },
                }
            }
        }
        let text = json!({ "vorgang": vorgang, "ereignisse": stapel }).to_string();
        if let Err(e) = melden(&edge, "signotec", &text).await {
            log::warn!("Pad-Meldung nicht angekommen: {e:#}");
        }
    }
}

/// Bon vom Server (ESC/POS) roh an den Windows-Drucker.
async fn drucken(a: &Auftrag) -> Ergebnis {
    let daten = match a.daten["daten"].as_str().map(|d| B64.decode(d)) {
        Some(Ok(d)) => d,
        _ => return fehler("Druckdaten fehlen oder sind nicht lesbar"),
    };
    let eingestellt = a.daten["drucker"].as_str().map(String::from);
    match tauri::async_runtime::spawn_blocking(move || drucker::drucken(eingestellt.as_deref(), &daten)).await {
        Ok(Ok(name)) => gut(json!({ "meldung": format!("gedruckt auf {name}") })),
        Ok(Err(e)) => fehler(format!("{e:#}")),
        Err(e) => fehler(e.to_string()),
    }
}

/// Setzt beim Ende des Tasks (auch beim Abbrechen) das Aus-Zeichen für den Thread des Lesers.
struct Aus(Arc<AtomicBool>);

impl Drop for Aus {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Karten vom Magnetkartenleser: ein Thread liest den COM-Anschluss, dieser Task meldet jede
/// Karte verschlüsselt an den Server. Die Nummer selbst kommt in kein Protokoll.
async fn karten(app: AppHandle) {
    let aus = Arc::new(AtomicBool::new(false));
    let _wache = Aus(aus.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let a = aus.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("magnetkarte".into())
        .spawn(move || magnetkarte::lesen(a, move |z| {
            let _ = tx.send(z);
        }))
    {
        log::warn!("Magnetkartenleser: Thread nicht gestartet: {e}");
        return;
    }
    while let Some(zeile) = rx.recv().await {
        let Some(edge) = app.state::<Zustand>().edge() else { return };
        match melden(&edge, "magnetkarte", &zeile).await {
            Ok(()) => log::info!("Karte gemeldet ({} Zeichen)", zeile.trim().len()),
            Err(e) => log::warn!("Karte nicht gemeldet: {e:#}"),
        }
    }
}

/// Eine Meldung verschlüsselt an den Server (`ich/ereignis`): Karte vom Leser, Stiftpunkte vom Pad.
async fn melden(edge: &Edge, typ: &str, inhalt: &str) -> anyhow::Result<()> {
    let schluessel = konfig::schluessel_lesen().ok_or_else(|| anyhow::anyhow!("Schlüssel fehlt – Platz neu koppeln"))?;
    // Dieselbe Kennung beim zweiten Versuch: kam der erste doch an, zählt der Server sie nicht doppelt
    let id = hex::encode(rand::random::<[u8; 16]>());
    let daten = krypto::verschluesseln(&schluessel, &id, inhalt.as_bytes())?;
    match edge.ereignis(typ, &id, &daten).await {
        Err(EdgeFehler::Netz(_)) => {
            tokio::time::sleep(Duration::from_millis(500)).await;
            edge.ereignis(typ, &id, &daten).await.map_err(|e| anyhow::anyhow!("{e}"))
        }
        r => r.map_err(|e| anyhow::anyhow!("{e}")),
    }
}

/// Bildschlüssel als base64 prüfen (für die Einrichtung).
pub fn schluessel_gueltig(s: &str) -> bool {
    B64.decode(s).map(|k| k.len() == 32).unwrap_or(false)
}
