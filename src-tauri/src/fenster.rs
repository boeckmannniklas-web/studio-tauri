//! Das eine Fenster der App.
//!
//! Es zeigt entweder eine eigene Seite (Einrichtung, Wartung, „keine Verbindung“) oder die
//! Oberfläche des Studio-Servers. Navigiert wird nur dorthin – Links nach draußen bleiben zu.
//! Ruft die Oberfläche `/kassenplatz/neu` auf (Platz-Token abgelaufen), holt der Agent ein
//! neues Ticket und öffnet sie damit wieder.
//!
//! Kiosk und Anzeige: Vollbild, immer im Vordergrund, Schließen gesperrt. Theke: Vollbild,
//! ebenfalls nur über das Wartungsmenü (Strg+Alt+S) zu beenden.
//!
//! Die Größe der Oberfläche stellt der Platz am Server ein (Zoom in Prozent); Strg +/− am Platz
//! bleibt aus, sonst verstellt sie jeder Kunde.

use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::Zustand;

pub const FENSTER: &str = "main";

/// Gegen Kontextmenü, Drucken, Quelltext und Entwicklerwerkzeuge – auch auf den Seiten des Servers.
const SCHUTZ: &str = r#"
(() => {
  document.addEventListener('contextmenu', (e) => e.preventDefault(), true);
  window.addEventListener('keydown', (e) => {
    const k = (e.key || '').toLowerCase();
    if (e.key === 'F12' || (e.ctrlKey && ['p', 'o', 'u', 'j'].includes(k)) || (e.ctrlKey && e.shiftKey && ['i', 'c'].includes(k))) {
      e.preventDefault();
    }
  }, true);
})();
"#;

/// Adresse einer eigenen Seite (unter Windows liefert WebView2 sie über http://tauri.localhost).
pub fn lokal(seite: &str) -> Url {
    #[cfg(windows)]
    let basis = "http://tauri.localhost/";
    #[cfg(not(windows))]
    let basis = "tauri://localhost/";
    Url::parse(basis).and_then(|b| b.join(seite)).expect("lokale Adresse")
}

fn ist_lokal(url: &Url) -> bool {
    matches!(url.scheme(), "tauri" | "about" | "data")
        || (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"))
}

fn ursprung(url: &Url) -> String {
    url.origin().ascii_serialization()
}

pub fn erstellen(app: &AppHandle) -> Result<WebviewWindow> {
    let pruefer = app.clone();
    #[allow(unused_mut)]
    let mut b = WebviewWindowBuilder::new(app, FENSTER, WebviewUrl::App("index.html".into()))
        .title("Studio Kassenplatz")
        .fullscreen(true)
        .decorations(false)
        .resizable(false)
        .zoom_hotkeys_enabled(false)
        .initialization_script(SCHUTZ)
        // Zoom je Seite: die Oberfläche des Servers in der Größe des Platzes, eigene Seiten wie gebaut
        .on_page_load(|w, p| {
            if p.event() == PageLoadEvent::Finished {
                let _ = w.set_zoom(if ist_lokal(p.url()) { 1.0 } else { platz_zoom(w.app_handle()) });
            }
        })
        .on_navigation(move |url| {
            if ist_lokal(url) {
                return true;
            }
            let z = pruefer.state::<Zustand>();
            let erlaubt = z.edge_ursprung.lock().map(|o| o.as_deref() == Some(ursprung(url).as_str())).unwrap_or(false);
            if !erlaubt {
                log::warn!("Navigation gesperrt: {url}");
                return false;
            }
            if url.path() == "/kassenplatz/neu" {
                let app = pruefer.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = zur_oberflaeche(&app).await {
                        log::warn!("Neues Ticket fehlgeschlagen: {e:#}");
                        zeige_lokal(&app, "offline.html");
                    }
                });
                return false;
            }
            true
        });
    #[cfg(windows)]
    {
        // Tauris eigene Vorgaben plus: kein Zoomen mit zwei Fingern, kein Wischen zurück
        b = b.additional_browser_args(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-pinch --overscroll-history-navigation=0",
        );
    }
    let fenster = b.build().context("Fenster anlegen")?;
    let app2 = app.clone();
    fenster.on_window_event(move |e| {
        if let WindowEvent::CloseRequested { api, .. } = e {
            if !app2.state::<Zustand>().darf_schliessen.load(Ordering::Relaxed) {
                api.prevent_close();
            }
        }
    });
    Ok(fenster)
}

/// Zoomfaktor der Oberfläche laut Platz (100 % = 1.0)
fn platz_zoom(app: &AppHandle) -> f64 {
    let z = app.state::<Zustand>().konfig.lock().unwrap().as_ref().map(|k| k.zoom).unwrap_or(100);
    f64::from(z.clamp(50, 200)) / 100.0
}

/// Geänderten Zoom sofort übernehmen, ohne die Seite neu zu laden.
pub fn zoom_anwenden(app: &AppHandle) {
    if let Some(f) = fenster(app) {
        if f.url().map(|u| !ist_lokal(&u)).unwrap_or(false) {
            let _ = f.set_zoom(platz_zoom(app));
        }
    }
}

pub fn fenster(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(FENSTER)
}

/// Kiosk und Anzeige bleiben im Vordergrund; die Theke darf hinter einen Dialog treten.
pub fn modus(app: &AppHandle, art: &str) {
    if let Some(f) = fenster(app) {
        let _ = f.set_fullscreen(true);
        let _ = f.set_always_on_top(art != "bedient");
    }
}

/// Für Dialoge des Systems (Benutzerkontensteuerung) Platz machen.
pub fn zuruecktreten(app: &AppHandle) {
    if let Some(f) = fenster(app) {
        let _ = f.set_always_on_top(false);
        let _ = f.set_fullscreen(false);
        let _ = f.minimize();
    }
}

pub fn zeige_lokal(app: &AppHandle, seite: &str) {
    if let Some(f) = fenster(app) {
        let _ = f.unminimize();
        let _ = f.navigate(lokal(seite));
    }
}

/// Oberfläche des Studio-Servers mit frischem Ticket öffnen.
pub async fn zur_oberflaeche(app: &AppHandle) -> Result<()> {
    let z = app.state::<Zustand>();
    let edge = z.edge().context("Nicht gekoppelt")?;
    let ticket = edge.ticket().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let ziel = Url::parse(&format!("{}/kassenplatz/start?ticket={ticket}", edge.basis))?;
    *z.edge_ursprung.lock().unwrap() = Some(ursprung(&ziel));
    let art = z.konfig.lock().unwrap().as_ref().map(|k| k.art.clone()).unwrap_or_default();
    modus(app, &art);
    if let Some(f) = fenster(app) {
        let _ = f.unminimize();
        f.navigate(ziel)?;
    }
    z.auf_oberflaeche.store(true, Ordering::Relaxed);
    Ok(())
}
