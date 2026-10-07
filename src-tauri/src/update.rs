//! Updates aus dem öffentlichen GitHub-Repo `studio-tauri` (latest.json, signiert).
//!
//! Geprüft wird zwei Minuten nach dem Start und dann alle sechs Stunden. Ein neues Update wird
//! gleich geladen, installiert aber erst nachts zwischen 3 und 4 Uhr – oder sofort über das
//! Wartungsmenü. Die App ist pro Benutzer installiert, ein Update braucht deshalb keine
//! Administratorrechte und läuft ohne Rückfrage.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Timelike;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::Zustand;

/// Prüfen und, wenn es eins gibt, laden. Liefert die neue Version.
pub async fn pruefen(app: &AppHandle) -> Result<Option<String>> {
    let z = app.state::<Zustand>();
    if let Some((u, _)) = z.update.lock().unwrap().as_ref() {
        return Ok(Some(u.version.clone()));
    }
    let Some(update) = app.updater().context("Updater")?.check().await.context("Nach Updates suchen")? else {
        return Ok(None);
    };
    let version = update.version.clone();
    log::info!("Update {version} gefunden, wird geladen");
    let daten = update.download(|_, _| {}, || {}).await.context("Update laden")?;
    *z.update.lock().unwrap() = Some((update, daten));
    Ok(Some(version))
}

/// Geladenes Update installieren – die App beendet sich dabei und startet neu.
pub fn installieren(app: &AppHandle) -> Result<bool> {
    let z = app.state::<Zustand>();
    let Some((update, daten)) = z.update.lock().unwrap().take() else { return Ok(false) };
    log::info!("Update {} wird installiert", update.version);
    z.darf_schliessen.store(true, std::sync::atomic::Ordering::Relaxed);
    update.install(daten).context("Update installieren")?;
    app.restart();
}

pub fn starten(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            if let Err(e) = pruefen(&app).await {
                log::warn!("Update-Prüfung: {e:#}");
            }
            // Sechs Stunden in Schritten von zehn Minuten: zwischendurch auf die Nacht achten
            for _ in 0..36 {
                tokio::time::sleep(Duration::from_secs(600)).await;
                let nachts = chrono::Local::now().hour() == 3;
                if nachts && !app.state::<Zustand>().in_wartung() {
                    if let Err(e) = installieren(&app) {
                        log::warn!("Update installieren: {e:#}");
                    }
                }
            }
        }
    });
}
