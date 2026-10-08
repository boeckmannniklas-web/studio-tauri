//! signotec-Unterschriftenpad am Platz (signoPAD-API für Windows, `STPadLib.dll`).
//!
//! Dieselbe Bibliothek wie am Studio-Server (dort die Linux-Ausgabe, signotec/helfer.py) – dieser
//! Teil macht hier, was dort der Hilfsprozess tut, und spricht dasselbe Protokoll: Der Server
//! schickt fertige Bildschirme (PNG) samt Unterschriftsfeld und Tasten, die App zeigt sie, meldet
//! jeden Stiftpunkt und am Ende Bild und SignData. Rendern, Durchsichtig-Machen und Ablegen am
//! Vertrag bleiben am Server.
//!
//! Die Bibliothek kommt mit der „signoPAD-API (64 Bit)“ von signotec (Treiber inklusive); gesucht
//! wird sie im Gerätepaket-Ordner und im Programmordner von signotec.

#[cfg(windows)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::{Receiver, Sender};
#[cfg(windows)]
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
#[cfg(windows)]
use anyhow::{bail, Context};
#[cfg(windows)]
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};

use super::pakete;

pub const DLL: &str = "STPadLib.dll";

/// Was in den Vorgang hineinkommt: vom Pad (Rückruf) und vom Server (Befehle).
pub enum Eingang {
    Punkt([i32; 4]),
    Taste(i32),
    Getrennt,
    Befehl(String),
    Ende,
}

/// Die Bibliothek: im Gerätepaket-Ordner, sonst dort, wo das Setup von signotec sie hinlegt.
pub fn dll_pfad() -> Option<PathBuf> {
    let eigen = pakete::ordner("signotec").join(DLL);
    if eigen.exists() {
        return Some(eigen);
    }
    let programme = std::env::var_os("ProgramFiles")?;
    let mut funde = Vec::new();
    suchen(&PathBuf::from(programme).join("signotec"), 5, &mut funde);
    // 64-Bit-Ordner vor allem anderen (die 32-Bit-DLL lädt ein 64-Bit-Programm nicht)
    funde.sort_by_key(|p| {
        let s = p.to_string_lossy().to_lowercase();
        (s.contains("x86") || s.contains("32bit") || s.contains("win32\\")) as u8
    });
    funde.into_iter().next()
}

fn suchen(ordner: &Path, tiefe: u32, funde: &mut Vec<PathBuf>) {
    let Ok(eintraege) = std::fs::read_dir(ordner) else { return };
    for e in eintraege.flatten() {
        let p = e.path();
        if p.is_dir() && tiefe > 0 {
            suchen(&p, tiefe - 1, funde);
        } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(DLL)) {
            funde.push(p);
        }
    }
}

pub fn zustand() -> (String, Option<String>) {
    if dll_pfad().is_some() {
        ("bereit".into(), None)
    } else {
        ("treiber_fehlt".into(), Some(
            "signoPAD-API von signotec fehlt – am Platz Wartung (Strg+Alt+S) → „Geräte einrichten“ lädt und installiert sie".into(),
        ))
    }
}

fn farbe(hex: &str) -> u32 {
    // '#RRGGBB' → COLORREF 0x00BBGGRR
    let h = hex.trim_start_matches('#');
    let wert = |i: usize| u32::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    wert(0) | (wert(2) << 8) | (wert(4) << 16)
}

fn modell(typ: i32) -> String {
    match typ {
        1 | 2 => "Sigma".into(),
        5 | 6 => "Zeta".into(),
        11 | 12 => "Omega".into(),
        15 | 16 => "Gamma".into(),
        21..=23 => "Delta".into(),
        31..=33 => "Alpha".into(),
        t => format!("Typ {t}"),
    }
}

#[cfg(windows)]
mod lib {
    use std::ffi::c_void;
    use std::path::Path;
    use std::sync::mpsc::Sender;

    use anyhow::{bail, Context, Result};
    use libloading::Library;

    use super::Eingang;

    type W = *const u16;
    type WM = *mut u16;
    pub type Rueckruf = unsafe extern "system" fn(i32, *const c_void, i32, *mut c_void);

    const CALLBACK_DISCONNECT: i32 = 0;
    const CALLBACK_HOTSPOT: i32 = 1;
    const CALLBACK_SIGNATURE: i32 = 4;

    pub fn breit(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn text(p: &[u16]) -> String {
        let n = p.iter().position(|&c| c == 0).unwrap_or(p.len());
        String::from_utf16_lossy(&p[..n])
    }

    /// Läuft im Thread der Bibliothek: nur einreihen, nie die Bibliothek selbst rufen.
    unsafe extern "system" fn rueckruf(ereignis: i32, daten: *const c_void, groesse: i32, eigen: *mut c_void) {
        if eigen.is_null() {
            return;
        }
        let sender = &*(eigen as *const Sender<Eingang>);
        let werte = |n: usize| -> Vec<i32> {
            if daten.is_null() || groesse < (n as i32) * 4 {
                return vec![];
            }
            std::slice::from_raw_parts(daten as *const i32, n).to_vec()
        };
        let _ = match ereignis {
            CALLBACK_SIGNATURE => match werte(4)[..] {
                [x, y, p, t] => sender.send(Eingang::Punkt([x, y, p, t])),
                _ => Ok(()),
            },
            CALLBACK_HOTSPOT => match werte(1)[..] {
                [id] => sender.send(Eingang::Taste(id)),
                _ => Ok(()),
            },
            CALLBACK_DISCONNECT => sender.send(Eingang::Getrennt),
            _ => Ok(()),
        };
    }

    macro_rules! funktionen {
        ($($name:ident: fn($($arg:ty),*) -> $ret:ty;)*) => {
            #[allow(non_snake_case)]
            pub struct Pad {
                _lib: Library,
                sender: *mut Sender<Eingang>,
                $(pub $name: unsafe extern "system" fn($($arg),*) -> $ret,)*
            }

            impl Pad {
                pub fn laden(pfad: &Path) -> Result<Pad> {
                    #[cfg(windows)]
                    let lib = unsafe {
                        libloading::os::windows::Library::load_with_flags(
                            pfad, libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH)
                    }.map(Library::from).with_context(|| format!("{} laden", pfad.display()))?;
                    unsafe {
                        Ok(Pad {
                            $($name: *lib.get(concat!(stringify!($name), "\0").as_bytes())
                                .with_context(|| concat!(stringify!($name), " fehlt in STPadLib.dll"))?,)*
                            _lib: lib,
                            sender: std::ptr::null_mut(),
                        })
                    }
                }
            }
        };
    }

    funktionen! {
        STControlGetVersion: fn(WM) -> i32;
        STControlGetErrorString: fn(WM, *mut i32, i32) -> i32;
        STControlSetAppName: fn(W) -> ();
        STControlSetCallback: fn(Option<Rueckruf>, *mut c_void) -> ();
        STControlExit: fn() -> ();
        STDeviceGetCount: fn() -> i32;
        STDeviceGetInfo: fn(WM, *mut i32, i32) -> i32;
        STDeviceGetVersion: fn(WM, i32) -> i32;
        STDeviceGetCapabilities: fn(i32) -> i32;
        STDeviceGetSensorResolution: fn(*mut i32, *mut i32, *mut i32, i32) -> i32;
        STDeviceOpen: fn(i32, i32) -> i32;
        STDeviceClose: fn(i32) -> i32;
        STDisplayGetWidth: fn() -> i32;
        STDisplayGetHeight: fn() -> i32;
        STDisplaySetTarget: fn(i32) -> i32;
        STDisplaySetImageFromFile: fn(i32, i32, W) -> i32;
        STDisplaySetImageFromStore: fn(i32) -> i32;
        STDisplayConfigPen: fn(i32, u32) -> i32;
        STDisplaySetStandbyImageFromFile: fn(W) -> i32;
        STDisplayConfigSlideShow: fn(W, i32) -> i32;
        STDisplayGetStandbyId: fn(WM, *mut i32) -> i32;
        STSensorSetSignRect: fn(i32, i32, i32, i32) -> i32;
        STSensorAddHotSpot: fn(i32, i32, i32, i32) -> i32;
        STSignatureStart: fn() -> i32;
        STSignatureRetry: fn() -> i32;
        STSignatureConfirm: fn() -> i32;
        STSignatureCancel: fn(i32) -> i32;
        STSignatureGetSignData: fn(*mut u8, *mut i32) -> i32;
        STSignatureSaveAsFileEx: fn(W, i32, i32, i32, i32, i32, u32, i32) -> i32;
    }

    impl Pad {
        pub fn rueckruf_setzen(&mut self, sender: Sender<Eingang>) {
            self.sender = Box::into_raw(Box::new(sender));
            unsafe { (self.STControlSetCallback)(Some(rueckruf), self.sender as *mut c_void) };
        }

        pub fn fehlertext(&self, code: i32) -> String {
            let mut puffer = [0u16; 256];
            let mut laenge = 256i32;
            if unsafe { (self.STControlGetErrorString)(puffer.as_mut_ptr(), &mut laenge, code) } < 0 {
                return "unbekannter Fehler".into();
            }
            text(&puffer)
        }

        /// Rückgabe prüfen: negativ → Fehler mit dem Text des Herstellers
        pub fn pruefen(&self, name: &str, rc: i32) -> Result<i32> {
            if rc < 0 {
                bail!("{name}: {} ({rc})", self.fehlertext(rc));
            }
            Ok(rc)
        }

        pub fn version(&self) -> String {
            let mut puffer = [0u16; 32];
            unsafe { (self.STControlGetVersion)(puffer.as_mut_ptr()) };
            text(&puffer)
        }

        pub fn info(&self, i: i32) -> Result<(String, i32, String, [i32; 3], i32)> {
            let mut sn = [0u16; 32];
            let mut typ = 0i32;
            self.pruefen("STDeviceGetInfo", unsafe { (self.STDeviceGetInfo)(sn.as_mut_ptr(), &mut typ, i) })?;
            let mut fw = [0u16; 32];
            self.pruefen("STDeviceGetVersion", unsafe { (self.STDeviceGetVersion)(fw.as_mut_ptr(), i) })?;
            let (mut w, mut h, mut p) = (0i32, 0i32, 0i32);
            self.pruefen("STDeviceGetSensorResolution",
                         unsafe { (self.STDeviceGetSensorResolution)(&mut w, &mut h, &mut p, i) })?;
            let caps = self.pruefen("STDeviceGetCapabilities", unsafe { (self.STDeviceGetCapabilities)(i) })?;
            Ok((text(&sn), typ, text(&fw), [w, h, p], caps))
        }

        pub fn signdata(&self) -> Result<Vec<u8>> {
            let mut n = 0i32;
            self.pruefen("STSignatureGetSignData", unsafe { (self.STSignatureGetSignData)(std::ptr::null_mut(), &mut n) })?;
            let mut puffer = vec![0u8; n.max(0) as usize];
            self.pruefen("STSignatureGetSignData", unsafe { (self.STSignatureGetSignData)(puffer.as_mut_ptr(), &mut n) })?;
            puffer.truncate(n.max(0) as usize);
            Ok(puffer)
        }

        pub fn standby_kennung(&self) -> Option<String> {
            let mut n = 0i32;
            if unsafe { (self.STDisplayGetStandbyId)(std::ptr::null_mut(), &mut n) } < 0 {
                return None;
            }
            let mut puffer = vec![0u16; n.max(1) as usize + 1];
            let mut n = puffer.len() as i32;
            if unsafe { (self.STDisplayGetStandbyId)(puffer.as_mut_ptr(), &mut n) } < 0 {
                return None;
            }
            Some(text(&puffer))
        }
    }

    impl Drop for Pad {
        fn drop(&mut self) {
            unsafe {
                (self.STControlSetCallback)(None, std::ptr::null_mut());
                (self.STControlExit)();
                if !self.sender.is_null() {
                    drop(Box::from_raw(self.sender));
                }
            }
        }
    }
}

#[cfg(windows)]
mod ablauf {
    use super::*;
    use super::lib::{breit, Pad};

    const TARGET_FOREGROUND: i32 = 0;
    const TARGET_BACKGROUND: i32 = 1;
    const FILETYPE_PNG: i32 = 1;
    const SIMG_SMOOTH: i32 = 0x0400;
    const CAP_COLORDISPLAY: i32 = 0x000001;
    const DANKE: Duration = Duration::from_millis(1500);

    pub struct Offen<'a> {
        pub pad: &'a Pad,
        pub geraet: Value,
        pub ordner: PathBuf,
    }

    impl Drop for Offen<'_> {
        fn drop(&mut self) {
            unsafe { (self.pad.STDeviceClose)(0) };
        }
    }

    pub fn geraete(pad: &Pad) -> Result<Vec<Value>> {
        let n = pad.pruefen("STDeviceGetCount", unsafe { (pad.STDeviceGetCount)() })?;
        let mut out = Vec::new();
        for i in 0..n {
            let (sn, typ, fw, [w, h, p], caps) = pad.info(i)?;
            out.push(json!({"index": i, "seriennummer": sn, "typ": typ, "modell": modell(typ), "firmware": fw,
                            "sensor": {"breite": w, "hoehe": h, "max_druck": p},
                            "farbdisplay": caps & CAP_COLORDISPLAY != 0}));
        }
        Ok(out)
    }

    pub fn oeffnen(pad: &Pad, loeschen: bool) -> Result<Offen<'_>> {
        let mut liste = geraete(pad)?;
        if liste.is_empty() {
            bail!("Kein Unterschriftenpad gefunden");
        }
        pad.pruefen("STDeviceOpen", unsafe { (pad.STDeviceOpen)(0, loeschen as i32) })?;
        let mut geraet = liste.remove(0);
        geraet["display"] = json!([unsafe { (pad.STDisplayGetWidth)() }, unsafe { (pad.STDisplayGetHeight)() }]);
        let ordner = std::env::temp_dir().join(format!("studio-signotec-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).context("Arbeitsordner anlegen")?;
        Ok(Offen { pad, geraet, ordner })
    }

    /// Im Hintergrundspeicher aufbauen, dann in einem Rutsch nach vorn (wie helfer.py).
    pub fn zeigen(o: &Offen, png_b64: &str) -> Result<()> {
        let pfad = o.ordner.join("bild.png");
        std::fs::write(&pfad, B64.decode(png_b64).context("Bild vom Server nicht lesbar")?)?;
        let w = breit(&pfad.to_string_lossy());
        let pad = o.pad;
        pad.pruefen("STDisplaySetTarget", unsafe { (pad.STDisplaySetTarget)(TARGET_BACKGROUND) })?;
        pad.pruefen("STDisplaySetImageFromFile", unsafe { (pad.STDisplaySetImageFromFile)(0, 0, w.as_ptr()) })?;
        pad.pruefen("STDisplaySetTarget", unsafe { (pad.STDisplaySetTarget)(TARGET_FOREGROUND) })?;
        pad.pruefen("STDisplaySetImageFromStore", unsafe { (pad.STDisplaySetImageFromStore)(TARGET_BACKGROUND) })?;
        Ok(())
    }

    fn rechteck(v: &Value) -> [i32; 4] {
        let z = |i: usize| v.get(i).and_then(Value::as_i64).unwrap_or(0) as i32;
        [z(0), z(1), z(2), z(3)]
    }

    pub fn unterschrift(pad: &Pad, befehl: &Value, melden: &dyn Fn(Value), eingang: &Receiver<Eingang>) -> Result<Vec<Value>> {
        let o = oeffnen(pad, true)?;
        let [breite, hoehe] = [o.geraet["display"][0].as_f64().unwrap_or(640.0), o.geraet["display"][1].as_f64().unwrap_or(480.0)];
        zeigen(&o, befehl["bild"].as_str().unwrap_or_default())?;
        let [x, y, w, h] = rechteck(&befehl["feld"]);
        pad.pruefen("STSensorSetSignRect", unsafe { (pad.STSensorSetSignRect)(x, y, w, h) })?;
        let mut tasten = HashMap::new();
        for t in befehl["tasten"].as_array().cloned().unwrap_or_default() {
            let [x, y, w, h] = rechteck(&t["rechteck"]);
            let id = pad.pruefen("STSensorAddHotSpot", unsafe { (pad.STSensorAddHotSpot)(x, y, w, h) })?;
            tasten.insert(id, t["id"].as_str().unwrap_or_default().to_string());
        }
        let stiftfarbe = farbe(befehl["stiftfarbe"].as_str().unwrap_or("#0d1a30"));
        let stiftbreite = befehl["stiftbreite"].as_i64().unwrap_or(3) as i32;
        pad.pruefen("STDisplayConfigPen", unsafe { (pad.STDisplayConfigPen)(stiftbreite, stiftfarbe) })?;
        pad.pruefen("STSignatureStart", unsafe { (pad.STSignatureStart)() })?;
        melden(json!({"ev": "bereit", "geraet": o.geraet, "feld": befehl["feld"], "bibliothek": pad.version()}));

        // Sensor → Displaypixel: die Oberfläche spiegelt in Displaypixeln
        let sx = breite / o.geraet["sensor"]["breite"].as_f64().unwrap_or(breite).max(1.0);
        let sy = hoehe / o.geraet["sensor"]["hoehe"].as_f64().unwrap_or(hoehe).max(1.0);
        let runden = |v: f64| (v * 10.0).round() / 10.0;
        let mut gezeichnet = 0u32;
        loop {
            let (aktion, durch) = match eingang.recv().unwrap_or(Eingang::Ende) {
                Eingang::Punkt([px, py, druck, zeit]) => {
                    if druck > 0 {
                        gezeichnet += 1;
                    }
                    melden(json!({"ev": "punkt", "x": runden(px as f64 * sx), "y": runden(py as f64 * sy), "p": druck, "t": zeit}));
                    continue;
                }
                Eingang::Getrennt => return Ok(vec![json!({"ev": "getrennt"})]),
                Eingang::Taste(id) => (tasten.get(&id).cloned().unwrap_or_default(), "pad"),
                Eingang::Befehl(cmd) => (cmd, "bildschirm"),
                Eingang::Ende => ("abbrechen".to_string(), "bildschirm"),
            };
            match aktion.as_str() {
                "neu" => {
                    pad.pruefen("STSignatureRetry", unsafe { (pad.STSignatureRetry)() })?;
                    gezeichnet = 0;
                    melden(json!({"ev": "neu"}));
                }
                "abbrechen" => {
                    unsafe { (pad.STSignatureCancel)(0) };
                    return Ok(vec![json!({"ev": "abgebrochen", "durch": durch})]);
                }
                "ok" | "bestaetigen" => {
                    if gezeichnet == 0 {
                        melden(json!({"ev": "leer"}));
                        continue;
                    }
                    let anzahl = pad.pruefen("STSignatureConfirm", unsafe { (pad.STSignatureConfirm)() })?;
                    let pfad = o.ordner.join("unterschrift.png");
                    let w = breit(&pfad.to_string_lossy());
                    pad.pruefen("STSignatureSaveAsFileEx", unsafe {
                        (pad.STSignatureSaveAsFileEx)(w.as_ptr(), 300, 0, 0, FILETYPE_PNG, 0, stiftfarbe, SIMG_SMOOTH)
                    })?;
                    let png = std::fs::read(&pfad).context("Unterschrift als Bild lesen")?;
                    // Die Biometriedaten sind ein Zusatz; ohne sie gilt die Unterschrift trotzdem
                    let signdata = pad.signdata().unwrap_or_else(|e| {
                        log::warn!("SignData nicht lesbar: {e:#}");
                        vec![]
                    });
                    if let Some(danke) = befehl["danke"].as_str() {
                        let _ = zeigen(&o, danke);
                        std::thread::sleep(DANKE);
                    }
                    return Ok(vec![json!({"ev": "fertig", "png": B64.encode(png), "signdata": B64.encode(signdata),
                                          "anzahl": anzahl, "durch": durch})]);
                }
                _ => {}
            }
        }
    }

    pub fn anzeigen(pad: &Pad, befehl: &Value, eingang: &Receiver<Eingang>) -> Result<Vec<Value>> {
        let o = oeffnen(pad, true)?;
        zeigen(&o, befehl["bild"].as_str().unwrap_or_default())?;
        let ende = Instant::now() + Duration::from_secs(befehl["sekunden"].as_u64().unwrap_or(8).min(60));
        while let Some(rest) = ende.checked_duration_since(Instant::now()) {
            match eingang.recv_timeout(rest) {
                Ok(Eingang::Befehl(c)) if c == "abbrechen" => break,
                Ok(Eingang::Ende) | Ok(Eingang::Getrennt) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => break,
                _ => {}
            }
        }
        Ok(vec![json!({"ev": "ok", "geraet": o.geraet})])
    }

    pub fn standby(pad: &Pad, befehl: &Value) -> Result<Vec<Value>> {
        let o = oeffnen(pad, false)?;
        if befehl["entfernen"].as_bool().unwrap_or(false) {
            let leer = breit("");
            pad.pruefen("STDisplayConfigSlideShow", unsafe { (pad.STDisplayConfigSlideShow)(leer.as_ptr(), 0) })?;
        } else {
            let pfad = o.ordner.join("standby.png");
            std::fs::write(&pfad, B64.decode(befehl["bild"].as_str().unwrap_or_default()).context("Standby-Bild")?)?;
            let w = breit(&pfad.to_string_lossy());
            // Schreibt in den Flash des Pads – dauert beim ersten Mal einige Sekunden
            pad.pruefen("STDisplaySetStandbyImageFromFile", unsafe { (pad.STDisplaySetStandbyImageFromFile)(w.as_ptr()) })?;
        }
        Ok(vec![json!({"ev": "ok", "geraet": o.geraet, "kennung": pad.standby_kennung()})])
    }

    pub fn info(pad: &Pad) -> Result<Vec<Value>> {
        // Mit Display-Größe – der Server rendert die Bildschirme passend
        let mut liste = geraete(pad)?;
        if !liste.is_empty() {
            let o = oeffnen(pad, false)?;
            liste[0]["display"] = o.geraet["display"].clone();
        }
        Ok(vec![json!({"ev": "info", "bibliothek": pad.version(), "geraete": liste})])
    }
}

/// Einen Vorgang am Pad ausführen (blockiert; im eigenen Thread laufen lassen). Zwischenstände gehen
/// an `melden`, das Ende kommt als Rückgabe – auch ein Fehler als Ereignis „fehler“.
pub fn vorgang(befehl: &Value, melden: &dyn Fn(Value), eingang: Receiver<Eingang>, rueckruf: Sender<Eingang>) -> Vec<Value> {
    match ausfuehren(befehl, melden, &eingang, rueckruf) {
        Ok(ende) => ende,
        Err(e) => vec![json!({"ev": "fehler", "meldung": format!("{e:#}")})],
    }
}

#[cfg(windows)]
fn ausfuehren(befehl: &Value, melden: &dyn Fn(Value), eingang: &Receiver<Eingang>, rueckruf: Sender<Eingang>) -> Result<Vec<Value>> {
    let pfad = dll_pfad().ok_or_else(|| anyhow!("signoPAD-API (64 Bit) von signotec ist auf diesem PC nicht installiert"))?;
    let mut pad = lib::Pad::laden(&pfad)?;
    pad.rueckruf_setzen(rueckruf);
    let app = lib::breit("Studio");
    unsafe { (pad.STControlSetAppName)(app.as_ptr()) };
    match befehl["cmd"].as_str().unwrap_or_default() {
        "unterschrift" => ablauf::unterschrift(&pad, befehl, melden, eingang),
        "anzeigen" => ablauf::anzeigen(&pad, befehl, eingang),
        "standby" => ablauf::standby(&pad, befehl),
        "info" => ablauf::info(&pad),
        andere => bail!("Unbekannter Befehl „{andere}“"),
    }
}

#[cfg(not(windows))]
fn ausfuehren(_befehl: &Value, _melden: &dyn Fn(Value), _eingang: &Receiver<Eingang>, _rueckruf: Sender<Eingang>) -> Result<Vec<Value>> {
    let _ = (farbe, modell);
    Err(anyhow!("Das Unterschriftenpad geht nur unter Windows"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farbe_als_colorref() {
        assert_eq!(farbe("#0d1a30"), 0x00301a0d);
        assert_eq!(farbe("#ffffff"), 0x00ffffff);
    }

    #[test]
    fn modelle() {
        assert_eq!(modell(11), "Omega");
        assert_eq!(modell(99), "Typ 99");
    }

    #[test]
    fn ohne_windows_ein_fehler_als_ereignis() {
        let (tx, rx) = std::sync::mpsc::channel();
        let ende = vorgang(&json!({"cmd": "info"}), &|_| {}, rx, tx);
        assert_eq!(ende[0]["ev"], "fehler");
    }
}
