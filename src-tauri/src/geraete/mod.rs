//! Geräte, die per USB an diesem PC stecken.
//!
//! Der Agent meldet sie dem Studio-Server (Herzschlag, „USB-Geräte suchen“). Nutzbar sind der
//! SecuGen-Fingerabdruckscanner, Magnetkartenleser an einem USB-Seriell-Wandler und Bondrucker
//! (über den Windows-Drucker). QR-Scanner im Tastaturmodus braucht die App nicht: sie tippen in die
//! Oberfläche, die erkennt den Scan selbst.

pub mod drucker;
pub mod magnetkarte;
pub mod pakete;
pub mod secugen;

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct UsbGeraet {
    /// finger | magnetkarte | qr | drucker – wie die Gerätefunktion am Server
    pub typ: &'static str,
    /// VID:PID
    pub usb_id: String,
    /// Wo es steckt – beim Kartenleser der COM-Anschluss
    #[serde(rename = "usb_anschluss", skip_serializing_if = "Option::is_none")]
    pub anschluss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seriennummer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub klasse: Option<&'static str>,
    /// bereit | treiber_fehlt | fehler | erkannt
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meldung: Option<String>,
}

const SECUGEN: u16 = 0x1162;
/// Hersteller von QR-/Barcode-Scannern und Bondruckern, die wir erkennen (noch ohne Unterstützung)
const QR: [u16; 5] = [0x0c2e /* Honeywell */, 0x05e0 /* Zebra */, 0x05f9 /* Datalogic */, 0x1eab /* Newland */, 0x26f1 /* RTscan */];
const DRUCKER: [u16; 3] = [0x04b8 /* Epson */, 0x0519 /* Star */, 0x0dd4 /* Custom */];

/// Produkt-ID → SDK-Klasse und Modell, wie am Studio-Server (secugen/erkennung.py)
pub fn secugen_modell(pid: u16) -> (Option<&'static str>, &'static str) {
    match pid {
        0x0320 => (Some("fdu03"), "SecuGen Hamster Plus"),
        0x0322 => (Some("fdu03"), "SecuGen Hamster Plus (SDU03M)"),
        0x1000 => (Some("fdu03"), "SecuGen Hamster Plus (SDU03P)"),
        0x0330 => (Some("fdu04"), "SecuGen Hamster IV"),
        0x2000 => (Some("fdu04"), "SecuGen Hamster IV (SDU04P)"),
        0x2200 => (Some("fdu05"), "SecuGen Hamster Pro 20"),
        0x2201 => (Some("fdu06"), "SecuGen Hamster Pro"),
        _ => (None, "SecuGen-Scanner"),
    }
}

pub fn suchen() -> Vec<UsbGeraet> {
    let Ok(alle) = nusb::list_devices() else { return vec![] };
    let mut out = Vec::new();
    for d in alle {
        let (vid, pid) = (d.vendor_id(), d.product_id());
        let usb_id = format!("{vid:04x}:{pid:04x}");
        let seriennummer = d.serial_number().map(String::from).filter(|s| !s.trim().is_empty());
        let produkt = d.product_string().map(String::from);
        if vid == SECUGEN {
            let (klasse, modell) = secugen_modell(pid);
            let (status, meldung) = secugen::zustand();
            out.push(UsbGeraet { typ: "finger", usb_id, anschluss: None, seriennummer, modell: Some(modell.into()), klasse, status, meldung });
        } else if let Some(chip) = magnetkarte::wandler(vid, pid) {
            let (status, meldung) = magnetkarte::zustand();
            let modell = Some(format!("Magnetkartenleser ({})", produkt.unwrap_or_else(|| chip.into())));
            out.push(UsbGeraet { typ: "magnetkarte", usb_id, anschluss: magnetkarte::stand().anschluss, seriennummer, modell,
                                 klasse: None, status, meldung });
        } else if QR.contains(&vid) {
            out.push(UsbGeraet { typ: "qr", usb_id, anschluss: None, seriennummer, modell: produkt, klasse: None, status: "bereit".into(),
                                 meldung: Some("Tastaturmodus – scannt direkt in die Oberfläche".into()) });
        } else if DRUCKER.contains(&vid) {
            let (status, meldung) = drucker::zustand();
            out.push(UsbGeraet { typ: "drucker", usb_id, anschluss: None, seriennummer, modell: produkt, klasse: None, status, meldung });
        }
    }
    out
}

/// Hersteller und Modell des PCs (für die Stammdaten des Erfassungsterminals).
pub fn rechner() -> (Option<String>, Option<String>) {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;
        if let Ok(bios) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("HARDWARE\\DESCRIPTION\\System\\BIOS") {
            let h: Option<String> = bios.get_value("SystemManufacturer").ok();
            let m: Option<String> = bios.get_value("SystemProductName").ok();
            return (h.filter(|s| !s.trim().is_empty()), m.filter(|s| !s.trim().is_empty()));
        }
        (None, None)
    }
    #[cfg(not(windows))]
    {
        (None, None)
    }
}
