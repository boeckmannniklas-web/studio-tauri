fn main() {
    // Eigene Befehle nur über das App-Manifest: Sie stehen dann unter Capabilities, und die
    // Capability „lokal“ gibt sie nur den Seiten dieser App frei – nie der Oberfläche des
    // Studio-Servers, die im selben Fenster läuft.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "einrichtung_stand",
            "edge_suchen",
            "koppeln",
            "starten",
            "geraete_einrichten",
            "wartung_pin",
            "wartung_info",
            "edge_aendern",
            "update_suchen",
            "update_installieren",
            "entkoppeln",
            "zurueck",
            "beenden",
        ]),
    ))
    .expect("tauri-build");
}
