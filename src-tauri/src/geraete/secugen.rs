//! SecuGen-Fingerabdruckscanner am Platz (FDx SDK Pro für Windows, `sgfplib.dll`).
//!
//! Der Platz nimmt nur das **Bild** auf. Merkmale und Vergleich rechnet der Studio-Server mit
//! demselben SDK, mit dem der Finger hinterlegt wurde (secugen/helfer.py `aus_bild`). So hängt
//! nichts davon ab, ob zwei SDK-Fassungen dieselben Merkmale finden.
//!
//! Die Herstellerbibliothek liegt nicht im Repo (sie ist nicht frei weiterzugeben). Sie kommt
//! als Gerätepaket vom Studio-Server nach `%ProgramData%\StudioKassenplatz\geraete\secugen`
//! (pakete.rs).

use std::sync::atomic::AtomicBool;

use anyhow::Result;

/// Ein aufgenommenes Bild: Graustufen, eine Zeile nach der anderen.
pub struct Aufnahme {
    pub bild: Vec<u8>,
    pub breite: u32,
    pub hoehe: u32,
    pub qualitaet: u32,
    pub seriennummer: Option<String>,
}

/// Graustufenbild als PNG (verlustfrei – der Server rechnet die Merkmale daraus).
pub fn png(a: &Aufnahme) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, a.breite, a.hoehe);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(&a.bild)?;
    }
    Ok(out)
}

#[cfg(windows)]
mod sdk {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use anyhow::{anyhow, bail, Context, Result};
    use libloading::os::windows::{Library, Symbol, LOAD_WITH_ALTERED_SEARCH_PATH};

    use super::Aufnahme;
    use crate::geraete::pakete;

    type Dw = u32;
    type H = *mut c_void;

    const SG_DEV_AUTO: Dw = 0xFF;
    const USB_AUTO_DETECT: Dw = 0x3BC + 1;
    /// „Kein Finger in der Zeit“
    const ZEIT_UM: Dw = 54;

    #[repr(C)]
    #[derive(Default)]
    struct Geraeteinfo {
        device_id: Dw,
        device_sn: [u8; 16],
        com_port: Dw,
        com_speed: Dw,
        image_width: Dw,
        image_height: Dw,
        contrast: Dw,
        brightness: Dw,
        gain: Dw,
        image_dpi: Dw,
        fw_version: Dw,
    }

    /// Je Prozess nur ein Vorgang am Scanner.
    static BELEGT: Mutex<()> = Mutex::new(());

    fn meldung(code: Dw) -> &'static str {
        match code {
            2 => "Funktion fehlgeschlagen",
            3 => "Ungültiger Parameter",
            5 | 6 | 7 => "SecuGen-Bibliothek nicht ladbar",
            51 => "Systemdatei nicht ladbar",
            52 => "Scanner ließ sich nicht initialisieren",
            53 => "Bilddaten unvollständig übertragen",
            54 => "Kein Finger aufgelegt",
            55 => "Kein Fingerabdruckscanner gefunden – Treiber installiert?",
            56 => "Gerätetreiber nicht ladbar",
            57 => "Bild unbrauchbar",
            59 => "Scanner ist schon geöffnet",
            _ => "Fehler am Scanner",
        }
    }

    struct Sdk {
        _lib: Library,
        create: Symbol<unsafe extern "system" fn(*mut H) -> Dw>,
        terminate: Symbol<unsafe extern "system" fn(H) -> Dw>,
        init: Symbol<unsafe extern "system" fn(H, Dw) -> Dw>,
        open: Symbol<unsafe extern "system" fn(H, Dw) -> Dw>,
        close: Symbol<unsafe extern "system" fn(H) -> Dw>,
        info: Symbol<unsafe extern "system" fn(H, *mut Geraeteinfo) -> Dw>,
        led: Symbol<unsafe extern "system" fn(H, i32) -> Dw>,
        bild: Symbol<unsafe extern "system" fn(H, *mut u8, Dw, *mut c_void, Dw) -> Dw>,
        qualitaet: Symbol<unsafe extern "system" fn(H, Dw, Dw, *mut u8, *mut Dw) -> Dw>,
    }

    impl Sdk {
        fn laden() -> Result<Sdk> {
            let pfad = pakete::ordner("secugen").join("sgfplib.dll");
            if !pfad.exists() {
                bail!("SecuGen-Treiber fehlt – am Platz über das Wartungsmenü „Geräte einrichten“");
            }
            unsafe {
                let lib = Library::load_with_flags(&pfad, LOAD_WITH_ALTERED_SEARCH_PATH).context("sgfplib.dll laden")?;
                macro_rules! f {
                    ($n:literal) => {
                        lib.get(concat!($n, "\0").as_bytes()).with_context(|| format!("{} fehlt in sgfplib.dll", $n))?
                    };
                }
                Ok(Sdk {
                    create: f!("SGFPM_Create"),
                    terminate: f!("SGFPM_Terminate"),
                    init: f!("SGFPM_Init"),
                    open: f!("SGFPM_OpenDevice"),
                    close: f!("SGFPM_CloseDevice"),
                    info: f!("SGFPM_GetDeviceInfo"),
                    led: f!("SGFPM_SetLedOn"),
                    bild: f!("SGFPM_GetImageEx"),
                    qualitaet: f!("SGFPM_GetImageQuality"),
                    _lib: lib,
                })
            }
        }
    }

    struct Offen<'a> {
        sdk: &'a Sdk,
        h: H,
        offen: bool,
    }

    impl Drop for Offen<'_> {
        fn drop(&mut self) {
            unsafe {
                if self.offen {
                    (self.sdk.led)(self.h, 0);
                    (self.sdk.close)(self.h);
                }
                (self.sdk.terminate)(self.h);
            }
        }
    }

    fn oeffnen(sdk: &Sdk) -> Result<(Offen<'_>, Geraeteinfo)> {
        let mut h: H = std::ptr::null_mut();
        let r = unsafe { (sdk.create)(&mut h) };
        if r != 0 {
            bail!("SecuGen-SDK: {} ({r})", meldung(r));
        }
        let mut o = Offen { sdk, h, offen: false };
        let r = unsafe { (sdk.init)(h, SG_DEV_AUTO) };
        if r != 0 {
            bail!("{} (Init {r})", meldung(r));
        }
        let mut r = unsafe { (sdk.open)(h, USB_AUTO_DETECT) };
        if r != 0 {
            r = unsafe { (sdk.open)(h, 0) };
        }
        if r != 0 {
            bail!("{} (OpenDevice {r})", meldung(r));
        }
        o.offen = true;
        let mut info = Geraeteinfo::default();
        let r = unsafe { (sdk.info)(h, &mut info) };
        if r != 0 || info.image_width == 0 || info.image_height == 0 {
            bail!("Kenndaten des Scanners nicht lesbar ({r})");
        }
        Ok((o, info))
    }

    fn sn(info: &Geraeteinfo) -> Option<String> {
        let ende = info.device_sn.iter().position(|b| *b == 0).unwrap_or(info.device_sn.len());
        let s = String::from_utf8_lossy(&info.device_sn[..ende]).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    pub fn da() -> bool {
        pakete::ordner("secugen").join("sgfplib.dll").exists()
    }

    pub fn testen() -> Result<String> {
        let _sperre = BELEGT.try_lock().map_err(|_| anyhow!("Der Scanner nimmt gerade auf"))?;
        let sdk = Sdk::laden()?;
        let (o, info) = oeffnen(&sdk)?;
        unsafe {
            (sdk.led)(o.h, 1);
        }
        std::thread::sleep(Duration::from_millis(400));
        Ok(format!(
            "Scanner antwortet · {}×{} @ {} dpi{}",
            info.image_width,
            info.image_height,
            info.image_dpi,
            sn(&info).map(|s| format!(" · SN {s}")).unwrap_or_default()
        ))
    }

    pub fn aufnehmen(zeit_ms: u64, qualitaet: u32, abbruch: &AtomicBool) -> Result<Aufnahme> {
        let _sperre = BELEGT.try_lock().map_err(|_| anyhow!("Der Scanner nimmt gerade auf"))?;
        let sdk = Sdk::laden()?;
        let (o, info) = oeffnen(&sdk)?;
        let (w, h) = (info.image_width, info.image_height);
        let mut puffer = vec![0u8; (w * h) as usize];
        unsafe {
            (sdk.led)(o.h, 1);
        }
        let ende = Instant::now() + Duration::from_millis(zeit_ms);
        loop {
            if abbruch.load(Ordering::Relaxed) {
                bail!("Abgebrochen");
            }
            // In kurzen Stücken warten, damit ein Abbruch schnell greift
            let r = unsafe { (sdk.bild)(o.h, puffer.as_mut_ptr(), 500, std::ptr::null_mut(), qualitaet) };
            if r == 0 {
                break;
            }
            if r != ZEIT_UM && r != 57 {
                bail!("{} ({r})", meldung(r));
            }
            if Instant::now() >= ende {
                bail!("Kein Finger aufgelegt");
            }
        }
        let mut q: Dw = 0;
        unsafe {
            (sdk.qualitaet)(o.h, w, h, puffer.as_mut_ptr(), &mut q);
        }
        Ok(Aufnahme { bild: puffer, breite: w, hoehe: h, qualitaet: q, seriennummer: sn(&info) })
    }
}

#[cfg(not(windows))]
mod sdk {
    use std::sync::atomic::AtomicBool;

    use anyhow::{bail, Result};

    use super::Aufnahme;

    pub fn da() -> bool {
        false
    }

    pub fn testen() -> Result<String> {
        bail!("Der SecuGen-Scanner am Platz wird nur unter Windows unterstützt")
    }

    pub fn aufnehmen(_zeit_ms: u64, _qualitaet: u32, _abbruch: &AtomicBool) -> Result<Aufnahme> {
        bail!("Der SecuGen-Scanner am Platz wird nur unter Windows unterstützt")
    }
}

/// Status für den Herzschlag: Ist die Herstellerbibliothek da?
pub fn zustand() -> (String, Option<String>) {
    if sdk::da() {
        ("bereit".into(), None)
    } else {
        ("treiber_fehlt".into(), Some("SecuGen-Treiber fehlt – „Geräte einrichten“ tippen".into()))
    }
}

pub fn testen() -> Result<String> {
    sdk::testen()
}

pub fn aufnehmen(zeit_ms: u64, qualitaet: u32, abbruch: &AtomicBool) -> Result<Aufnahme> {
    sdk::aufnehmen(zeit_ms, qualitaet, abbruch)
}
