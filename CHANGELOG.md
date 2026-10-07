# Changelog

## 0.1.0 – 2026-10-07

Erste Fassung (Stufe 1 der Kassenplätze).

- Einrichtung: Studio-Server per mDNS bzw. im eigenen /24-Netz finden, Koppelcode oder QR-Code,
  Gerätetoken in der Windows-Anmeldeinformationsverwaltung, Autostart.
- Vollbild mit Navigationsschutz; Kiosk und Anzeige immer im Vordergrund, Schließen nur über das
  Wartungsmenü (Strg+Alt+S, PIN vom Server).
- Geräte-Agent: Herzschlag alle 30 s (Version, PC, USB-Geräte), Aufträge per Long-Poll.
  Bleibt der Server stumm, zeigt der Platz „keine Verbindung“ und sucht ihn am Mandanten wieder.
- SecuGen-Fingerabdruckscanner am Platz: Aufnahme, verschlüsselte Übergabe des Bildes an den Server.
  Treiber und Bibliothek als Gerätepaket vom Server.
- QR-Scanner und Bondrucker werden erkannt und gemeldet; Unterstützung folgt.
- Updates aus GitHub-Releases, signiert, nachts oder über das Wartungsmenü.
