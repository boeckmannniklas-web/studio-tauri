//! Studio-Kassenplatz: Theke, Self-Service-Kiosk und Verfügbarkeitsanzeige als Windows-App.
//!
//! Die App ist eine dünne Hülle um die Oberfläche des Studio-Servers (Edge). Sie koppelt den
//! PC mit einem Einmalcode, öffnet die Oberfläche im Vollbild und bindet Geräte ein, die per
//! USB am PC stecken (Geräte-Agent). Die Oberfläche selbst bekommt keinen Zugriff auf die App:
//! Alles Native läuft zwischen Agent und Server (docs/KASSENPLAETZE.md im Studio-Repo).

mod agent;
mod edge;
mod fenster;
mod geraete;
mod konfig;
mod krypto;
mod suche;
mod update;
mod wartung;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct Zustand {
    pub ordner: PathBuf,
    pub konfig: Mutex<Option<konfig::Konfig>>,
    /// Ursprung der Oberfläche, zu der navigiert werden darf (http://host[:port])
    pub edge_ursprung: Mutex<Option<String>>,
    /// Bis wann das Wartungsmenü nach richtiger PIN offen bleibt
    pub wartung_bis: Mutex<Option<Instant>>,
    pub darf_schliessen: AtomicBool,
    pub auf_oberflaeche: AtomicBool,
    pub offline: AtomicBool,
    pub agent: Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>,
    pub abbrueche: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub zuletzt: Mutex<Option<edge::PlatzInfo>>,
    pub update: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
    /// Hinweis für die Einrichtungsseite (z. B. „wurde entkoppelt“)
    pub hinweis: Mutex<Option<String>>,
}

impl Zustand {
    pub fn edge(&self) -> Option<edge::Edge> {
        let basis = self.konfig.lock().unwrap().as_ref()?.edge.clone();
        Some(edge::Edge::neu(&basis, &konfig::token_lesen()?))
    }

    pub fn gekoppelt(&self) -> bool {
        self.konfig.lock().unwrap().is_some() && konfig::token_lesen().is_some()
    }

    pub fn in_wartung(&self) -> bool {
        self.wartung_bis.lock().unwrap().is_some_and(|b| b > Instant::now())
    }
}

pub fn run() {
    let wartung_taste = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyS);

    tauri::Builder::default()
        // Ein zweiter Start holt nur das Fenster nach vorn
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(f) = fenster::fenster(app) {
                let _ = f.unminimize();
                let _ = f.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                    file_name: Some("kassenplatz".into()),
                }))
                .max_file_size(2_000_000)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, taste, ereignis| {
                    if taste == &wartung_taste && ereignis.state() == ShortcutState::Pressed {
                        wartung::oeffnen(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let ordner = app.path().app_config_dir()?;
            let konfig = konfig::Konfig::laden(&ordner);
            app.manage(Zustand {
                ordner,
                konfig: Mutex::new(konfig),
                edge_ursprung: Mutex::new(None),
                wartung_bis: Mutex::new(None),
                darf_schliessen: AtomicBool::new(false),
                auf_oberflaeche: AtomicBool::new(false),
                offline: AtomicBool::new(false),
                agent: Mutex::new(Vec::new()),
                abbrueche: Mutex::new(HashMap::new()),
                zuletzt: Mutex::new(None),
                update: Mutex::new(None),
                hinweis: Mutex::new(None),
            });
            fenster::erstellen(app.handle())?;
            if let Err(e) = app.global_shortcut().register(wartung_taste) {
                log::warn!("Strg+Alt+S nicht belegbar: {e}");
            }
            let handle = app.handle().clone();
            if handle.state::<Zustand>().gekoppelt() {
                agent::starten(&handle);
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = fenster::zur_oberflaeche(&handle).await {
                        log::warn!("Start: {e:#}");
                        handle.state::<Zustand>().offline.store(true, Ordering::Relaxed);
                        fenster::zeige_lokal(&handle, "offline.html");
                    }
                });
            }
            update::starten(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wartung::einrichtung_stand,
            wartung::edge_suchen,
            wartung::koppeln,
            wartung::starten,
            wartung::geraete_einrichten,
            wartung::wartung_pin,
            wartung::wartung_info,
            wartung::edge_aendern,
            wartung::update_suchen,
            wartung::update_installieren,
            wartung::entkoppeln,
            wartung::zurueck,
            wartung::beenden,
        ])
        .run(tauri::generate_context!())
        .expect("Studio Kassenplatz konnte nicht starten");
}
