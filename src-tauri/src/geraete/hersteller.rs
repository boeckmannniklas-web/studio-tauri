//! Setups von Herstellern, die die App selbst bei ihnen lädt – statt sie als Gerätepaket vom
//! Studio-Server zu bekommen. So liegt keine fremde Datei bei uns, und die Adresse ist die offizielle.
//!
//! Geladen wird nur, was hier mit Adresse, Größe und SHA-256 steht; eine andere Datei wird nicht
//! ausgeführt. Installiert wird still mit Administratorrechten (Benutzerkontensteuerung) – deshalb
//! wie die Gerätepakete nur in der Einrichtung bzw. im Wartungsmenü.
//!
//! Kommt eine neue Version: Adresse, Größe und Prüfsumme hier ändern, App-Release.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use super::pakete;

pub struct Setup {
    pub name: &'static str,
    pub url: &'static str,
    pub groesse: u64,
    pub sha256: &'static str,
    /// Stille Installation
    pub argumente: &'static str,
    /// Für den Fall, dass es so nicht klappt: die Download-Seite des Herstellers
    pub seite: &'static str,
}

/// signoPAD-API 9.0.1 (64 Bit) – Pad-Treiber und `STPadLib.dll`. InstallShield: `/s` still, `/v"…"` an
/// msiexec; alle Features außer Doku, Quelltext und Beispielen (Installationsanleitung signotec 3.2.2).
pub const SIGNOTEC: Setup = Setup {
    name: "signoPAD-API 9.0.1 (64 Bit)",
    url: "https://backend.signotec.com/wp-content/uploads/2025/11/signoPAD-API_9.0.1_64Bit.exe",
    groesse: 141_452_792,
    sha256: "905c3d143595b9fe516728a90d532350832cf06870d8aacabdc5dbd29cdc4935",
    argumente: r#"/s /v"ADDLOCAL=ALL REMOVE=Documentation,SourceCode,Samples /qn""#,
    seite: "https://www.signotec.com/en/help-centre/signopad-api/#downloads",
};

/// Seiten, die die App im Browser öffnen darf (Links aus der Geräteliste)
pub const SEITEN: [&str; 2] = ["https://www.signotec.com/", "https://secugen.com/"];

fn ablage() -> PathBuf {
    pakete::ordner("downloads")
}

/// Laden und prüfen. Liegt die richtige Datei schon da, wird sie nicht noch einmal geladen.
pub async fn laden(s: &Setup) -> Result<PathBuf> {
    let datei = ablage().join(s.url.rsplit('/').next().unwrap_or("setup.exe"));
    if datei.exists() && pruefsumme(&datei)? == s.sha256 {
        return Ok(datei);
    }
    std::fs::create_dir_all(ablage()).context("Download-Ordner anlegen")?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("StudioKassenplatz/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(30 * 60))
        .build()?;
    let mut antwort = client.get(s.url).send().await.with_context(|| format!("{} laden", s.name))?;
    if !antwort.status().is_success() {
        bail!("{} nicht ladbar (HTTP {}) – bitte von {} herunterladen", s.name, antwort.status().as_u16(), s.seite);
    }
    let teil = datei.with_extension("teil");
    let mut f = std::fs::File::create(&teil).context("Download-Datei anlegen")?;
    let mut hash = Sha256::new();
    let mut gelesen = 0u64;
    while let Some(stueck) = antwort.chunk().await.context("Download unterbrochen")? {
        gelesen += stueck.len() as u64;
        if gelesen > s.groesse {
            break;
        }
        hash.update(&stueck);
        f.write_all(&stueck)?;
    }
    drop(f);
    let summe = hex::encode(hash.finalize());
    if gelesen != s.groesse || summe != s.sha256 {
        let _ = std::fs::remove_file(&teil);
        bail!("{} hat nicht die erwartete Größe/Prüfsumme – nicht installiert. Bitte von {} herunterladen.", s.name, s.seite);
    }
    std::fs::rename(&teil, &datei).context("Download ablegen")?;
    Ok(datei)
}

fn pruefsumme(pfad: &std::path::Path) -> Result<String> {
    let mut f = std::fs::File::open(pfad)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut f, &mut hash)?;
    Ok(hex::encode(hash.finalize()))
}

/// Still installieren (fragt nach Administratorrechten). Blockiert bis zum Ende.
pub fn installieren(s: &Setup, datei: &std::path::Path) -> Result<String> {
    let ordner = datei.parent().map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let code = pakete::als_admin(&datei.to_string_lossy(), s.argumente, &ordner)?;
    // 3010: Neustart empfohlen – das Pad geht meist trotzdem gleich
    if code != 0 && code != 3010 {
        bail!("{} endete mit Code {code} – bitte von {} installieren", s.name, s.seite);
    }
    Ok(format!("{} installiert{}", s.name, if code == 3010 { " (Neustart empfohlen)" } else { "" }))
}

/// Eine Herstellerseite im Browser öffnen – nur die freigegebenen.
pub fn seite_oeffnen(url: &str) -> Result<()> {
    if !SEITEN.iter().any(|s| url.starts_with(s)) {
        bail!("Diese Adresse öffnet die App nicht");
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (verb, ziel) = (w("open"), w(url));
        let rc = unsafe {
            ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), ziel.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
        };
        if (rc as isize) <= 32 {
            bail!("Browser ließ sich nicht öffnen ({})", rc as isize);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        bail!("Nur unter Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_freigegebene_seiten() {
        assert!(seite_oeffnen("https://evil.example/").is_err());
        assert!(SIGNOTEC.seite.starts_with(SEITEN[0]) && SIGNOTEC.url.starts_with("https://backend.signotec.com/"));
        assert_eq!(SIGNOTEC.sha256.len(), 64);
    }
}
