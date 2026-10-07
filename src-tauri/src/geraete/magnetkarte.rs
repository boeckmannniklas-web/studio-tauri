//! Magnetkartenleser am Platz – etwa der Glancetron am USB-Seriell-Wandler CP2102 (unter Windows
//! ein COM-Anschluss).
//!
//! Der Leser wartet auf keinen Befehl: zieht jemand eine Karte durch, schickt er eine Zeile (die
//! Ziffern der Spur, dann CR LF). Ein Hintergrund-Thread hält den Anschluss offen und gibt jede
//! Zeile weiter; der Agent meldet sie verschlüsselt an den Studio-Server (`ich/ereignis`). Wem die
//! Karte gehört, weiß nur der Server – hier landet die Nummer weder im Protokoll noch auf der Platte.
//!
//! Wie am Server (magnetkarte/leser.py): nach dem Öffnen DTR/RTS kurz aus und wieder an, sonst
//! bleibt der Glancetron stumm; ohne Zeilenende gilt eine Zeile nach 300 ms Ruhe als fertig.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// USB-Seriell-Wandler, an denen Kartenleser hängen (wie am Server, magnetkarte/erkennung.py)
const WANDLER: [(u16, u16, &str); 4] = [
    (0x10c4, 0xea60, "Silicon Labs CP210x"),
    (0x0403, 0x6001, "FTDI FT232"),
    (0x067b, 0x2303, "Prolific PL2303"),
    (0x1a86, 0x7523, "WCH CH340"),
];

pub const BAUDRATE: u32 = 9600;
const ZEILE_RUHE: Duration = Duration::from_millis(300);
const NEUVERSUCH: Duration = Duration::from_secs(3);
const LEITUNGEN_AUS: Duration = Duration::from_millis(500);
/// Kürzere Zeilen sind Störungen (wie MINDESTLAENGE am Server, dort nach dem Auspacken der Spur)
const MINDESTLAENGE: usize = 3;

pub fn wandler(vid: u16, pid: u16) -> Option<&'static str> {
    WANDLER.iter().find(|(v, p, _)| *v == vid && *p == pid).map(|(_, _, n)| *n)
}

#[derive(Clone, Debug, Default)]
pub struct Stand {
    /// COM-Anschluss, solange er offen ist
    pub anschluss: Option<String>,
    pub fehler: Option<String>,
    pub karten: u64,
    /// läuft der Thread überhaupt (nur bei gekoppeltem Platz)
    pub laeuft: bool,
}

static STAND: Mutex<Stand> = Mutex::new(Stand { anschluss: None, fehler: None, karten: 0, laeuft: false });

pub fn stand() -> Stand {
    STAND.lock().unwrap().clone()
}

fn setzen(f: impl FnOnce(&mut Stand)) {
    f(&mut STAND.lock().unwrap());
}

/// (status, meldung) für die Geräteliste: bereit | fehler | treiber_fehlt
pub fn zustand() -> (String, Option<String>) {
    let s = stand();
    if s.anschluss.is_some() {
        return ("bereit".into(), None);
    }
    if let Some(f) = s.fehler {
        return ("fehler".into(), Some(f));
    }
    if !s.laeuft {
        return ("erkannt".into(), Some("Wird nach dem Koppeln verbunden".into()));
    }
    ("treiber_fehlt".into(), Some(
        "Kein COM-Anschluss für den Leser – Treiber des USB-Seriell-Wandlers fehlt (z. B. CP210x von Silicon Labs, \
         kommt meist über Windows Update)".into(),
    ))
}

/// Der COM-Anschluss eines bekannten Wandlers, falls Windows einen angelegt hat.
fn anschluss_finden() -> Option<(String, &'static str)> {
    let ports = serialport::available_ports().ok()?;
    ports.into_iter().find_map(|p| match p.port_type {
        serialport::SerialPortType::UsbPort(u) => wandler(u.vid, u.pid).map(|chip| (p.port_name, chip)),
        _ => None,
    })
}

/// Thread-Schleife: Anschluss suchen, öffnen, anstoßen, Zeilen lesen – bis `aus` gesetzt ist.
/// Jede Zeile geht an `zeile` (im Thread des Lesers; der Empfänger leitet sie weiter).
pub fn lesen(aus: Arc<AtomicBool>, zeile: impl Fn(String)) {
    setzen(|s| s.laeuft = true);
    let mut gemeldet: Option<String> = None;
    while !aus.load(Ordering::Relaxed) {
        let Some((name, chip)) = anschluss_finden() else {
            setzen(|s| {
                s.anschluss = None;
                s.fehler = None;
            });
            schlafen(&aus, NEUVERSUCH);
            continue;
        };
        match oeffnen(&name) {
            Ok(mut port) => {
                if gemeldet.as_deref() != Some(&name) {
                    log::info!("Magnetkartenleser an {name} ({chip}) verbunden");
                    gemeldet = Some(name.clone());
                }
                setzen(|s| {
                    s.anschluss = Some(name.clone());
                    s.fehler = None;
                });
                if let Err(e) = zeilen_lesen(port.as_mut(), &aus, &zeile) {
                    log::warn!("Magnetkartenleser an {name}: {e}");
                    setzen(|s| {
                        s.anschluss = None;
                        s.fehler = Some(format!("Kartenleser an {name} nicht mehr erreichbar: {e}"));
                    });
                    gemeldet = None;
                }
            }
            Err(e) => {
                let m = format!("{name} lässt sich nicht öffnen: {e} (von einem anderen Programm belegt?)");
                if gemeldet.as_deref() != Some(m.as_str()) {
                    log::warn!("Magnetkartenleser: {m}");
                    gemeldet = Some(m.clone());
                }
                setzen(|s| {
                    s.anschluss = None;
                    s.fehler = Some(m);
                });
            }
        }
        schlafen(&aus, NEUVERSUCH);
    }
    setzen(|s| {
        s.anschluss = None;
        s.laeuft = false;
    });
}

fn schlafen(aus: &AtomicBool, wie_lange: Duration) {
    let bis = Instant::now() + wie_lange;
    while !aus.load(Ordering::Relaxed) && Instant::now() < bis {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Öffnen (8N1, ohne Flusskontrolle) und anstoßen: DTR/RTS aus, warten, wieder an, Eingang leeren.
fn oeffnen(name: &str) -> serialport::Result<Box<dyn serialport::SerialPort>> {
    let mut port = serialport::new(name, BAUDRATE)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(serialport::StopBits::One)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(100))
        .open()?;
    // Kennt der Anschluss keine Steuerleitungen, geht es ohne
    let _ = port.write_data_terminal_ready(false);
    let _ = port.write_request_to_send(false);
    std::thread::sleep(LEITUNGEN_AUS);
    let _ = port.write_data_terminal_ready(true);
    let _ = port.write_request_to_send(true);
    std::thread::sleep(Duration::from_millis(200));
    let _ = port.clear(serialport::ClearBuffer::Input);
    Ok(port)
}

fn zeilen_lesen(port: &mut dyn serialport::SerialPort, aus: &AtomicBool, zeile: &impl Fn(String)) -> std::io::Result<()> {
    let mut puffer = Zeilenpuffer::default();
    let mut roh = [0u8; 256];
    while !aus.load(Ordering::Relaxed) {
        match port.read(&mut roh) {
            Ok(0) => return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Gerät abgezogen")),
            Ok(n) => {
                for z in puffer.dazu(&roh[..n], Instant::now()) {
                    fertig(z, zeile);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(e),
        }
        if let Some(z) = puffer.ruhe(Instant::now()) {
            fertig(z, zeile);
        }
    }
    Ok(())
}

fn fertig(z: String, zeile: &impl Fn(String)) {
    if z.trim().chars().count() < MINDESTLAENGE {
        return;
    }
    setzen(|s| s.karten += 1);
    zeile(z);
}

/// Bytes → Zeilen: CR oder LF beendet eine Zeile, ebenso 300 ms Ruhe.
#[derive(Default)]
struct Zeilenpuffer {
    daten: Vec<u8>,
    zuletzt: Option<Instant>,
}

impl Zeilenpuffer {
    fn dazu(&mut self, bytes: &[u8], jetzt: Instant) -> Vec<String> {
        let mut aus = Vec::new();
        for &b in bytes {
            if b == b'\r' || b == b'\n' {
                if !self.daten.is_empty() {
                    aus.push(self.nehmen());
                }
            } else if self.daten.len() < 512 {
                self.daten.push(b);
            }
        }
        self.zuletzt = Some(jetzt);
        aus
    }

    fn ruhe(&mut self, jetzt: Instant) -> Option<String> {
        match self.zuletzt {
            Some(t) if !self.daten.is_empty() && jetzt.duration_since(t) >= ZEILE_RUHE => Some(self.nehmen()),
            _ => None,
        }
    }

    fn nehmen(&mut self) -> String {
        let z = String::from_utf8_lossy(&self.daten).into_owned();
        self.daten.clear();
        z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeilen_mit_ende_und_nach_ruhe() {
        let t = Instant::now();
        let mut p = Zeilenpuffer::default();
        assert_eq!(p.dazu(b";60123", t), Vec::<String>::new());
        assert_eq!(p.dazu(b"45678?\r\n%B1?\n", t), vec![";6012345678?".to_string(), "%B1?".to_string()]);
        assert_eq!(p.dazu(b"6012", t), Vec::<String>::new());
        assert_eq!(p.ruhe(t + Duration::from_millis(100)), None);
        assert_eq!(p.ruhe(t + Duration::from_millis(350)), Some("6012".to_string()));
        assert_eq!(p.ruhe(t + Duration::from_secs(5)), None);
    }

    #[test]
    fn wandler_wie_am_server() {
        assert_eq!(wandler(0x10c4, 0xea60), Some("Silicon Labs CP210x"));
        assert_eq!(wandler(0x1162, 0x0322), None);
    }
}
