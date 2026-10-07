//! Was der Platz über sich behält: der Studio-Server, an den er gekoppelt ist, und wer er dort ist.
//!
//! Die Datei `kassenplatz.json` im App-Ordner enthält nichts Geheimes. Gerätetoken und
//! Bildschlüssel liegen in der Windows-Anmeldeinformationsverwaltung (keyring), nicht im
//! Klartext auf der Platte.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DATEI: &str = "kassenplatz.json";
const DIENST: &str = "Studio Kassenplatz";
const TOKEN: &str = "geraetetoken";
const SCHLUESSEL: &str = "bildschluessel";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Konfig {
    /// z. B. `http://192.168.178.30` – ohne Pfad, ohne Schrägstrich am Ende
    pub edge: String,
    pub platz_id: String,
    pub tenant_id: String,
    #[serde(default)]
    pub edge_name: String,
    #[serde(default)]
    pub platz_name: String,
    #[serde(default)]
    pub platz_nr: String,
    /// bedient | kiosk | anzeige
    #[serde(default)]
    pub art: String,
    #[serde(default)]
    pub ausrichtung: String,
}

impl Konfig {
    fn pfad(ordner: &Path) -> PathBuf {
        ordner.join(DATEI)
    }

    pub fn laden(ordner: &Path) -> Option<Self> {
        let roh = fs::read_to_string(Self::pfad(ordner)).ok()?;
        serde_json::from_str(&roh).ok()
    }

    pub fn speichern(&self, ordner: &Path) -> Result<()> {
        fs::create_dir_all(ordner).context("App-Ordner anlegen")?;
        fs::write(Self::pfad(ordner), serde_json::to_vec_pretty(self)?).context("kassenplatz.json schreiben")
    }

    pub fn loeschen(ordner: &Path) {
        let _ = fs::remove_file(Self::pfad(ordner));
    }
}

fn eintrag(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(DIENST, name).context("Anmeldeinformationsverwaltung")
}

pub fn token_lesen() -> Option<String> {
    eintrag(TOKEN).ok()?.get_password().ok()
}

pub fn schluessel_lesen() -> Option<String> {
    eintrag(SCHLUESSEL).ok()?.get_password().ok()
}

pub fn geheimnisse_speichern(token: &str, schluessel: &str) -> Result<()> {
    eintrag(TOKEN)?.set_password(token).context("Gerätetoken speichern")?;
    eintrag(SCHLUESSEL)?.set_password(schluessel).context("Bildschlüssel speichern")?;
    Ok(())
}

pub fn geheimnisse_loeschen() {
    for name in [TOKEN, SCHLUESSEL] {
        if let Ok(e) = eintrag(name) {
            let _ = e.delete_credential();
        }
    }
}

/// Adresse normalisieren: `studio.local`, `192.168.178.30:8080`, `http://…/` → `http://host[:port]`.
pub fn edge_normal(eingabe: &str) -> Result<String> {
    let roh = eingabe.trim().trim_end_matches('/');
    let mit = if roh.starts_with("http://") || roh.starts_with("https://") {
        roh.to_string()
    } else {
        format!("http://{roh}")
    };
    let url = url::Url::parse(&mit).context("Adresse nicht lesbar")?;
    let host = url.host_str().context("Adresse ohne Rechnername")?;
    Ok(match url.port() {
        Some(p) => format!("{}://{}:{}", url.scheme(), host, p),
        None => format!("{}://{}", url.scheme(), host),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adressen() {
        assert_eq!(edge_normal("studio.local").unwrap(), "http://studio.local");
        assert_eq!(edge_normal(" 192.168.178.30:8080/ ").unwrap(), "http://192.168.178.30:8080");
        assert_eq!(edge_normal("http://192.168.178.30/kasse").unwrap(), "http://192.168.178.30");
    }
}
