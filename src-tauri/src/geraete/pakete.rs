//! Gerätepakete: Treiber und Herstellerbibliothek eines Geräts, geliefert vom Studio-Server.
//!
//! Herstellerdateien (SecuGen) dürfen nicht ins öffentliche Repo dieser App. Der Studio-Server
//! liefert sie als ZIP mit `pruefsummen.json` aus. Ausgepackt wird nach
//! `%ProgramData%\StudioKassenplatz\geraete\<typ>`; ein Treiber wird mit Administratorrechten
//! installiert – deshalb nur in der Einrichtung oder im Wartungsmenü, wenn jemand dabei ist,
//! nie im laufenden Kiosk (die Rückfrage der Benutzerkontensteuerung käme sonst unbemerkt).
//!
//! `pruefsummen.json`:
//! ```json
//! { "dateien": { "sgfplib.dll": "<sha256>", … },
//!   "treiber": ["treiber/sgfdu03.inf"],                         // per pnputil, oder
//!   "installer": { "datei": "setup.exe", "argumente": "/S" } }   // Herstellerinstaller
//! ```

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Installer {
    datei: String,
    #[serde(default)]
    argumente: String,
}

#[derive(Debug, Deserialize)]
struct Pruefsummen {
    dateien: BTreeMap<String, String>,
    #[serde(default)]
    treiber: Vec<String>,
    #[serde(default)]
    installer: Option<Installer>,
}

pub fn ordner(typ: &str) -> PathBuf {
    let basis = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    basis.join("StudioKassenplatz").join("geraete").join(typ)
}

fn sicherer_name(name: &str) -> Result<&str> {
    if name.is_empty() || name.contains("..") || name.starts_with('/') || name.starts_with('\\') || name.contains(':') {
        bail!("Ungültiger Dateiname im Gerätepaket: {name}");
    }
    Ok(name)
}

/// ZIP prüfen und auspacken. Liefert den Zielordner und was danach zu installieren ist.
fn auspacken(typ: &str, zip: &[u8]) -> Result<(PathBuf, Pruefsummen)> {
    let mut archiv = zip::ZipArchive::new(std::io::Cursor::new(zip)).context("Gerätepaket ist kein ZIP")?;
    let pruef: Pruefsummen = {
        let mut f = archiv.by_name("pruefsummen.json").context("pruefsummen.json fehlt im Gerätepaket")?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        serde_json::from_str(&s).context("pruefsummen.json nicht lesbar")?
    };
    let ziel = ordner(typ);
    std::fs::create_dir_all(&ziel).with_context(|| format!("{} anlegen", ziel.display()))?;
    for (name, soll) in &pruef.dateien {
        let name = sicherer_name(name)?;
        let mut f = archiv.by_name(name).with_context(|| format!("{name} fehlt im Gerätepaket"))?;
        let mut inhalt = Vec::new();
        f.read_to_end(&mut inhalt)?;
        let ist = hex::encode(Sha256::digest(&inhalt));
        if !ist.eq_ignore_ascii_case(soll) {
            bail!("Prüfsumme von {name} stimmt nicht – Paket beschädigt");
        }
        let pfad = ziel.join(name);
        if let Some(p) = pfad.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&pfad, inhalt).with_context(|| format!("{} schreiben", pfad.display()))?;
    }
    Ok((ziel, pruef))
}

#[cfg(windows)]
pub(crate) fn als_admin(programm: &str, argumente: &str, ordner: &Path) -> Result<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn w(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }
    let verb = w("runas".as_ref());
    let datei = w(programm.as_ref());
    let args = w(argumente.as_ref());
    let dir = w(ordner.as_os_str());
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = datei.as_ptr();
        info.lpParameters = args.as_ptr();
        info.lpDirectory = dir.as_ptr();
        info.nShow = SW_HIDE as i32;
        if ShellExecuteExW(&mut info) == 0 {
            bail!("Installation abgelehnt oder nicht gestartet (Benutzerkontensteuerung?)");
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code: u32 = 1;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}

#[cfg(not(windows))]
pub(crate) fn als_admin(_programm: &str, _argumente: &str, _ordner: &Path) -> Result<u32> {
    bail!("Treiber werden nur unter Windows installiert")
}

/// Paket auspacken und Treiber installieren. Fragt nach Administratorrechten.
pub fn einrichten(typ: &str, zip: &[u8]) -> Result<String> {
    let (ziel, pruef) = auspacken(typ, zip)?;
    if let Some(inst) = &pruef.installer {
        let datei = ziel.join(sicherer_name(&inst.datei)?);
        let code = als_admin(&datei.to_string_lossy(), &inst.argumente, &ziel)?;
        if code != 0 && code != 3010 {
            bail!("Herstellerinstaller endete mit Code {code}");
        }
    }
    for inf in &pruef.treiber {
        let pfad = ziel.join(sicherer_name(inf)?);
        let code = als_admin("pnputil.exe", &format!("/add-driver \"{}\" /install", pfad.display()), &ziel)?;
        // 3010: Neustart empfohlen; 259: kein neueres Gerät betroffen – beides ist in Ordnung
        if code != 0 && code != 3010 && code != 259 {
            bail!("Treiber {inf} ließ sich nicht installieren (pnputil {code})");
        }
    }
    Ok(format!("Gerätepaket „{typ}“ eingerichtet ({} Dateien)", pruef.dateien.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn falsche_pruefsumme_wird_abgelehnt() {
        let mut puffer = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut puffer));
            let opt = zip::write::SimpleFileOptions::default();
            z.start_file("pruefsummen.json", opt).unwrap();
            z.write_all(br#"{"dateien": {"a.dll": "00"}}"#).unwrap();
            z.start_file("a.dll", opt).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        assert!(auspacken("test", &puffer).unwrap_err().to_string().contains("Prüfsumme"));
        assert!(sicherer_name("../boese.dll").is_err());
    }
}
