//! Mit Windows starten – ja oder nein.
//!
//! Der Installer fragt bei der ersten Installation und legt die Antwort unter
//! `HKCU\Software\Studio Kassenplatz`, Wert `Autostart` (1/0) ab (windows/hooks.nsh). Gibt die
//! Kassenverwaltung für den Platz Ja oder Nein vor, gilt das; „wie bei der Installation“ schickt der
//! Server als `null`. Ohne Eintrag (Installer vor 0.5.0) startet die App mit Windows – wie bisher.

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::Zustand;

/// Die Wahl beim Installieren.
pub fn bei_installation() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Studio Kassenplatz") {
            if let Ok(v) = k.get_value::<u32, _>("Autostart") {
                return v != 0;
            }
        }
    }
    true
}

/// Autostart so setzen, wie Server bzw. Installer es wollen – nur, wenn es anders ist.
pub fn anwenden(app: &AppHandle) {
    let vorgabe = app.state::<Zustand>().konfig.lock().unwrap().as_ref().and_then(|k| k.autostart);
    let soll = vorgabe.unwrap_or_else(bei_installation);
    let start = app.autolaunch();
    if start.is_enabled().ok() == Some(soll) {
        return;
    }
    let erg = if soll { start.enable() } else { start.disable() };
    match erg {
        Ok(()) => log::info!("Autostart {}", if soll { "an" } else { "aus" }),
        Err(e) => log::warn!("Autostart: {e}"),
    }
}
