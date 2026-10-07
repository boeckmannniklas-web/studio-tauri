//! Bondrucker am Platz – über den Windows-Druckspooler, roh.
//!
//! Den Bon rendert der Studio-Server (ESC/POS, nach dem Bon-Design) und schickt die Bytes als
//! Auftrag „drucken“. Hier gehen sie unverändert („RAW“) an einen Windows-Drucker – so drucken
//! Kassensysteme auf Thermodrucker, ohne dass Windows etwas umsetzt. Der Drucker muss in Windows
//! eingerichtet sein (Treiber des Herstellers, etwa Epson Advanced Printer Driver).
//!
//! Welcher Drucker: der am Server eingestellte, sonst der erste, dessen Name nach Bondrucker aussieht,
//! sonst der Standarddrucker.

use anyhow::{bail, Result};

/// Woran man einen Bondrucker am Namen erkennt
const BON_NAMEN: [&str; 9] = ["tm-", "epson", "star", "bon", "receipt", "pos", "thermal", "80mm", "58mm"];

pub fn sieht_nach_bon_aus(name: &str) -> bool {
    let n = name.to_lowercase();
    BON_NAMEN.iter().any(|b| n.contains(b)) && !n.contains("pdf") && !n.contains("xps")
}

/// Der Drucker für einen Bon: eingestellt, sonst nach Namen, sonst Standard.
pub fn waehlen(eingestellt: Option<&str>, alle: &[String], standard: Option<String>) -> Option<String> {
    if let Some(e) = eingestellt.filter(|e| !e.trim().is_empty()) {
        return Some(e.to_string());
    }
    alle.iter().find(|n| sieht_nach_bon_aus(n)).cloned().or(standard)
}

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use anyhow::{bail, Result};
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Graphics::Printing::{
        ClosePrinter, EndDocPrinter, EndPagePrinter, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW,
        StartDocPrinterW, StartPagePrinter, WritePrinter, DOC_INFO_1W, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
        PRINTER_INFO_4W,
    };

    fn breit(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    unsafe fn lesen(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        OsString::from_wide(std::slice::from_raw_parts(p, n)).to_string_lossy().into_owned()
    }

    pub fn liste() -> Vec<String> {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut noetig, mut anzahl) = (0u32, 0u32);
        unsafe {
            EnumPrintersW(flags, std::ptr::null(), 4, std::ptr::null_mut(), 0, &mut noetig, &mut anzahl);
            if noetig == 0 {
                return vec![];
            }
            // u64-ausgerichtet: die Einträge enthalten Zeiger
            let mut puffer = vec![0u64; (noetig as usize).div_ceil(8)];
            if EnumPrintersW(flags, std::ptr::null(), 4, puffer.as_mut_ptr() as *mut u8, noetig, &mut noetig, &mut anzahl) == 0 {
                return vec![];
            }
            let eintraege = std::slice::from_raw_parts(puffer.as_ptr() as *const PRINTER_INFO_4W, anzahl as usize);
            eintraege.iter().map(|e| lesen(e.pPrinterName)).filter(|n| !n.is_empty()).collect()
        }
    }

    pub fn standard() -> Option<String> {
        let mut n = 0u32;
        unsafe {
            GetDefaultPrinterW(std::ptr::null_mut(), &mut n);
            if n == 0 {
                return None;
            }
            let mut puffer = vec![0u16; n as usize];
            if GetDefaultPrinterW(puffer.as_mut_ptr(), &mut n) == 0 {
                return None;
            }
            Some(lesen(puffer.as_ptr())).filter(|s| !s.is_empty())
        }
    }

    struct Offen(HANDLE);

    impl Drop for Offen {
        fn drop(&mut self) {
            unsafe {
                ClosePrinter(self.0);
            }
        }
    }

    pub fn roh(drucker: &str, daten: &[u8]) -> Result<()> {
        let name = breit(drucker);
        let mut h: HANDLE = std::ptr::null_mut();
        unsafe {
            if OpenPrinterW(name.as_ptr(), &mut h, std::ptr::null()) == 0 {
                bail!("Drucker „{drucker}“ lässt sich nicht öffnen ({})", std::io::Error::last_os_error());
            }
            let offen = Offen(h);
            let mut dokument = breit("Studio Kassenplatz – Bon");
            let mut art = breit("RAW");
            let info = DOC_INFO_1W { pDocName: dokument.as_mut_ptr(), pOutputFile: std::ptr::null_mut(), pDatatype: art.as_mut_ptr() };
            if StartDocPrinterW(offen.0, 1, &info) == 0 {
                bail!("Druckauftrag nicht angenommen ({})", std::io::Error::last_os_error());
            }
            StartPagePrinter(offen.0);
            let mut geschrieben = 0u32;
            let ok = WritePrinter(offen.0, daten.as_ptr() as *const _, daten.len() as u32, &mut geschrieben);
            EndPagePrinter(offen.0);
            EndDocPrinter(offen.0);
            if ok == 0 || geschrieben as usize != daten.len() {
                bail!("Nur {geschrieben} von {} Byte geschrieben ({})", daten.len(), std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn liste() -> Vec<String> {
    win::liste()
}

#[cfg(windows)]
pub fn standard() -> Option<String> {
    win::standard()
}

#[cfg(not(windows))]
pub fn liste() -> Vec<String> {
    vec![]
}

#[cfg(not(windows))]
pub fn standard() -> Option<String> {
    None
}

/// Rohdaten drucken. Gibt den Namen des Druckers zurück.
pub fn drucken(eingestellt: Option<&str>, daten: &[u8]) -> Result<String> {
    let Some(name) = waehlen(eingestellt, &liste(), standard()) else {
        bail!("Kein Drucker in Windows eingerichtet – den Bondrucker mit dem Treiber des Herstellers installieren");
    };
    #[cfg(windows)]
    {
        win::roh(&name, daten)?;
        Ok(name)
    }
    #[cfg(not(windows))]
    {
        let _ = daten;
        bail!("Drucken geht nur unter Windows ({name})")
    }
}

/// Zustand für die Geräteliste: ein USB-Bondrucker ist bereit, wenn Windows einen passenden Drucker kennt.
pub fn zustand() -> (String, Option<String>) {
    let alle = liste();
    match alle.iter().find(|n| sieht_nach_bon_aus(n)) {
        Some(n) => ("bereit".into(), Some(format!("Windows-Drucker „{n}“"))),
        None if !alle.is_empty() => ("bereit".into(), Some("Kein Bondrucker am Namen erkannt – am Server den Windows-Drucker wählen".into())),
        None => ("treiber_fehlt".into(), Some("In Windows ist kein Drucker eingerichtet – Treiber des Herstellers installieren".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drucker_waehlen() {
        let alle = vec!["Microsoft Print to PDF".to_string(), "EPSON TM-T20II Receipt".to_string()];
        assert_eq!(waehlen(Some("Mein Drucker"), &alle, None).as_deref(), Some("Mein Drucker"));
        assert_eq!(waehlen(None, &alle, Some("Microsoft Print to PDF".into())).as_deref(), Some("EPSON TM-T20II Receipt"));
        assert_eq!(waehlen(Some(" "), &["HP LaserJet".to_string()], Some("HP LaserJet".into())).as_deref(), Some("HP LaserJet"));
        assert!(!sieht_nach_bon_aus("EPSON Print to PDF"));
    }
}
