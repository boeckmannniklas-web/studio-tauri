//! Befehle der eigenen Seiten: Einrichtung (koppeln) und Wartungsmenü (Strg+Alt+S).
//!
//! Nur die Seiten dieser App dürfen sie aufrufen (Capability „lokal“, build.rs) – nie die
//! Oberfläche des Studio-Servers. Ist der Platz gekoppelt, öffnet erst die Wartungs-PIN (am
//! Server festgelegt, dort geprüft) für zehn Minuten Änderungen, Treiber, Updates und Beenden.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::edge::{self, Agentinfo, EdgeFehler};
use crate::geraete::{self, hersteller, pakete, UsbGeraet};
use crate::{agent, fenster, konfig, suche, update, Zustand};

const WARTUNG_OFFEN: Duration = Duration::from_secs(600);

#[derive(Serialize)]
pub struct Stand {
    gekoppelt: bool,
    edge: Option<String>,
    edge_name: Option<String>,
    platz_name: Option<String>,
    platz_nr: Option<String>,
    art: Option<String>,
    version: &'static str,
    hinweis: Option<String>,
    wartung_offen: bool,
    wartungs_pin: bool,
    offline: bool,
    update: Option<String>,
    usb: Vec<UsbGeraet>,
}

fn stand(app: &AppHandle) -> Stand {
    let z = app.state::<Zustand>();
    let k = z.konfig.lock().unwrap().clone();
    let gekoppelt = z.gekoppelt();
    let update = z.update.lock().unwrap().as_ref().map(|(u, _)| u.version.clone());
    let hinweis = z.hinweis.lock().unwrap().clone();
    let wartungs_pin = z.zuletzt.lock().unwrap().as_ref().map(|p| p.wartungs_pin).unwrap_or(false);
    Stand {
        gekoppelt,
        edge: k.as_ref().map(|k| k.edge.clone()),
        edge_name: k.as_ref().map(|k| k.edge_name.clone()),
        platz_name: k.as_ref().map(|k| k.platz_name.clone()),
        platz_nr: k.as_ref().map(|k| k.platz_nr.clone()),
        art: k.as_ref().map(|k| k.art.clone()),
        version: env!("CARGO_PKG_VERSION"),
        hinweis,
        wartung_offen: z.in_wartung(),
        wartungs_pin,
        offline: z.offline.load(Ordering::Relaxed),
        update,
        usb: geraete::suchen(),
    }
}

fn freigegeben(app: &AppHandle) -> Result<(), String> {
    let z = app.state::<Zustand>();
    if !z.gekoppelt() || z.in_wartung() {
        Ok(())
    } else {
        Err("Wartung gesperrt – bitte zuerst die Wartungs-PIN eingeben.".into())
    }
}

/// Strg+Alt+S: Wartungsmenü (bzw. die Einrichtung, solange nicht gekoppelt).
pub fn oeffnen(app: &AppHandle) {
    let z = app.state::<Zustand>();
    z.auf_oberflaeche.store(false, Ordering::Relaxed);
    fenster::zeige_lokal(app, if z.gekoppelt() { "wartung.html" } else { "index.html" });
}

#[tauri::command]
pub async fn einrichtung_stand(app: AppHandle) -> Result<Stand, String> {
    Ok(tauri::async_runtime::spawn_blocking(move || stand(&app)).await.map_err(|e| e.to_string())?)
}

#[tauri::command]
pub async fn wartung_info(app: AppHandle) -> Result<Stand, String> {
    einrichtung_stand(app).await
}

#[tauri::command]
pub async fn edge_suchen() -> Result<Vec<suche::EdgeFund>, String> {
    Ok(suche::suchen().await)
}

/// Eingabe aus dem Feld: Code oder der gescannte QR-Code aus dem Koppeldialog
/// (`studio-kassenplatz://koppeln?edge=…&code=…`).
fn zerlegen(edge: &str, code: &str) -> (String, String) {
    for feld in [code, edge] {
        if let Ok(u) = url::Url::parse(feld.trim()) {
            if u.scheme() == "studio-kassenplatz" {
                let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
                if let (Some(e), Some(c)) = (q.get("edge"), q.get("code")) {
                    return (e.clone(), c.clone());
                }
            }
        }
    }
    (edge.to_string(), code.to_string())
}

#[tauri::command]
pub async fn koppeln(app: AppHandle, edge: String, code: String) -> Result<Stand, String> {
    freigegeben(&app)?;
    let (edge, code) = zerlegen(&edge, &code);
    let basis = konfig::edge_normal(&edge).map_err(|e| format!("{e:#}"))?;
    let hardware = machine_uid::get().unwrap_or_else(|_| "unbekannt".into());
    let info = Agentinfo { bildschirm: agent::bildschirm(&app), ..agent::agentinfo() };
    let a = edge::koppeln(&basis, &code, &hardware, &info).await.map_err(|e| match e {
        EdgeFehler::Server(403, _) => "Der Code ist falsch oder abgelaufen – in der Kassenverwaltung einen neuen erzeugen.".to_string(),
        EdgeFehler::Server(404, _) => "Unter dieser Adresse läuft kein Studio-Server mit der App „Kassenplätze“.".to_string(),
        andere => andere.to_string(),
    })?;
    if !agent::schluessel_gueltig(&a.schluessel) {
        return Err("Der Studio-Server hat keinen gültigen Schlüssel geschickt.".into());
    }
    konfig::geheimnisse_speichern(&a.token, &a.schluessel).map_err(|e| format!("{e:#}"))?;
    let k = konfig::Konfig {
        edge: basis, platz_id: a.platz.id.clone(), tenant_id: a.edge.tenant_id.clone(), edge_name: a.edge.name.clone(),
        platz_name: a.platz.name.clone(), platz_nr: a.platz.nr.clone(), art: a.platz.art.clone(),
        ausrichtung: a.platz.ausrichtung.clone(), zoom: a.platz.zoom, autostart: a.platz.autostart,
    };
    let z = app.state::<Zustand>();
    k.speichern(&z.ordner).map_err(|e| format!("{e:#}"))?;
    *z.konfig.lock().unwrap() = Some(k);
    *z.zuletzt.lock().unwrap() = Some(a.platz);
    *z.hinweis.lock().unwrap() = None;
    // Nach dem Koppeln darf der Techniker noch Treiber einrichten, ohne PIN
    *z.wartung_bis.lock().unwrap() = Some(Instant::now() + WARTUNG_OFFEN);
    crate::autostart::anwenden(&app);
    agent::starten(&app);
    einrichtung_stand(app).await
}

#[tauri::command]
pub async fn starten(app: AppHandle) -> Result<(), String> {
    *app.state::<Zustand>().wartung_bis.lock().unwrap() = None;
    fenster::zur_oberflaeche(&app).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn zurueck(app: AppHandle) -> Result<(), String> {
    let z = app.state::<Zustand>();
    *z.wartung_bis.lock().unwrap() = None;
    if z.gekoppelt() {
        fenster::zur_oberflaeche(&app).await.map_err(|e| format!("{e:#}"))
    } else {
        fenster::zeige_lokal(&app, "index.html");
        Ok(())
    }
}

#[tauri::command]
pub async fn wartung_pin(app: AppHandle, pin: String) -> Result<bool, String> {
    let z = app.state::<Zustand>();
    let Some(edge) = z.edge() else { return Ok(true) };
    let ok = edge.wartung(&pin).await.map_err(|e| e.to_string())?;
    if ok {
        *z.wartung_bis.lock().unwrap() = Some(Instant::now() + WARTUNG_OFFEN);
    }
    Ok(ok)
}

/// Treiber und Herstellerbibliothek für erkannte USB-Geräte holen und einrichten.
#[tauri::command]
pub async fn geraete_einrichten(app: AppHandle) -> Result<String, String> {
    freigegeben(&app)?;
    let edge = app.state::<Zustand>().edge().ok_or("Erst koppeln")?;
    let usb = tauri::async_runtime::spawn_blocking(geraete::suchen).await.map_err(|e| e.to_string())?;
    let finger = usb.iter().any(|g| g.typ == "finger");
    let pad = usb.iter().any(|g| g.typ == "unterschrift" && g.status == "treiber_fehlt");
    if !finger && !pad {
        return Err("Kein USB-Gerät, das etwas braucht (Fingerabdruckscanner von SecuGen, Unterschriftenpad von signotec).".into());
    }
    // Erst alles laden – dann erst die Rückfragen der Benutzerkontensteuerung
    let mut meldungen = Vec::new();
    let paket = if finger {
        match edge.geraetepaket("secugen").await {
            Ok(zip) => Some(zip),
            Err(e) => {
                meldungen.push(format!("Fingerabdruckscanner: {e}"));
                None
            }
        }
    } else {
        None
    };
    let setup = if pad {
        match hersteller::laden(&hersteller::SIGNOTEC).await {
            Ok(datei) => Some(datei),
            Err(e) => {
                meldungen.push(format!("Unterschriftenpad: {e:#}"));
                None
            }
        }
    } else {
        None
    };
    let mut gelungen = false;
    if paket.is_some() || setup.is_some() {
        // Die Benutzerkontensteuerung fragt gleich nach – das Fenster macht dafür Platz
        fenster::zuruecktreten(&app);
        let erg = tauri::async_runtime::spawn_blocking(move || {
            let mut out = Vec::new();
            if let Some(zip) = paket {
                out.push(pakete::einrichten("secugen", &zip).map_err(|e| format!("Fingerabdruckscanner: {e:#}")));
            }
            if let Some(datei) = setup {
                out.push(hersteller::installieren(&hersteller::SIGNOTEC, &datei).map_err(|e| format!("Unterschriftenpad: {e:#}")));
            }
            out
        })
        .await
        .map_err(|e| e.to_string())?;
        let art = app.state::<Zustand>().konfig.lock().unwrap().as_ref().map(|k| k.art.clone()).unwrap_or_default();
        fenster::modus(&app, &art);
        if let Some(f) = fenster::fenster(&app) {
            let _ = f.unminimize();
            let _ = f.set_focus();
        }
        for e in erg {
            match e {
                Ok(m) => {
                    gelungen = true;
                    meldungen.push(m);
                }
                Err(m) => meldungen.push(m),
            }
        }
    }
    if gelungen { Ok(meldungen.join(" · ")) } else { Err(meldungen.join(" · ")) }
}

/// Download-Seite eines Herstellers im Browser (Link in der Geräteliste) – nur freigegebene Seiten.
#[tauri::command]
pub async fn seite_oeffnen(app: AppHandle, url: String) -> Result<(), String> {
    freigegeben(&app)?;
    fenster::zuruecktreten(&app);
    hersteller::seite_oeffnen(&url).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn edge_aendern(app: AppHandle, edge: String) -> Result<Stand, String> {
    freigegeben(&app)?;
    let basis = konfig::edge_normal(&edge).map_err(|e| format!("{e:#}"))?;
    let kopf = edge::kopf(&basis, Duration::from_secs(5)).await.map_err(|e| e.to_string())?;
    let z = app.state::<Zustand>();
    {
        let mut k = z.konfig.lock().unwrap();
        let k = k.as_mut().ok_or("Nicht gekoppelt")?;
        if k.tenant_id != kopf.tenant_id {
            return Err(format!("Unter {basis} läuft ein anderes Studio ({}). Für einen Wechsel bitte entkoppeln und neu koppeln.", kopf.name));
        }
        k.edge = basis;
        k.speichern(&z.ordner).map_err(|e| format!("{e:#}"))?;
    }
    agent::starten(&app);
    einrichtung_stand(app).await
}

#[tauri::command]
pub async fn update_suchen(app: AppHandle) -> Result<Option<String>, String> {
    freigegeben(&app)?;
    update::pruefen(&app).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn update_installieren(app: AppHandle) -> Result<bool, String> {
    freigegeben(&app)?;
    update::pruefen(&app).await.map_err(|e| format!("{e:#}"))?;
    update::installieren(&app).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn entkoppeln(app: AppHandle) -> Result<(), String> {
    freigegeben(&app)?;
    if let Some(edge) = app.state::<Zustand>().edge() {
        if let Err(e) = edge.entkoppeln().await {
            log::warn!("Entkoppeln am Server: {e}");
        }
    }
    agent::entkoppelt(&app, "Der Platz ist entkoppelt.");
    Ok(())
}

#[tauri::command]
pub fn beenden(app: AppHandle) -> Result<(), String> {
    freigegeben(&app)?;
    app.state::<Zustand>().darf_schliessen.store(true, Ordering::Relaxed);
    app.exit(0);
    Ok(())
}
