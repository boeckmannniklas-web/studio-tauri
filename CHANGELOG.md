# Changelog

## 0.5.0 – 2026-10-10

- **Mit Windows starten – ja oder nein:** Der Installer fragt bei der ersten Installation, ob der Platz
  mit Windows startet (empfohlen für Kiosk und Anzeige). Updates fragen nie – ohne frühere Antwort bleibt
  es beim Start mit Windows. Die Antwort steht unter `HKCU\Software\Studio Kassenplatz` (`Autostart`).
- Die Kassenverwaltung kann es je Platz vorgeben (Ja / Nein / wie bei der Installation, auch beim
  Anlegen des Platzes); die App übernimmt die Vorgabe beim nächsten Abgleich. Braucht einen Studio-Server
  mit der Gestaltung der Kassenplätze – ältere Server schicken nichts, dann gilt die Wahl im Installer.
- Die App meldet Auflösung und Windows-Skalierung ihres Bildschirms; die Kassenverwaltung übernimmt sie für
  die Vorschau („Vom Platz übernehmen“).

## 0.4.3 – 2026-10-08

- QR-/Barcode-Scanner von SUNMI (Blink Scanning Box, USB-ID 324F) werden erkannt und stehen in der
  Geräteliste auf „bereit“. Scanner anderer Hersteller erkennt die App zusätzlich am Produktnamen
  („Scanner“, „Barcode“ …). Wie bisher im Tastaturmodus: Der Scanner tippt in die Oberfläche, Kasse und
  Kiosk erkennen den Scan selbst.

## 0.4.2 – 2026-10-08

- Fingerabdruckscanner mit einem Klick: „Geräte einrichten“ installiert alle SecuGen-Treiber des
  Gerätepakets mit einem einzigen `pnputil`-Aufruf – die Benutzerkontensteuerung fragt nur noch einmal
  (vorher je Scannerfamilie). Braucht einen Studio-Server, der das Gerätepaket mitbringt
  (SDK 4.3.1 + Treiber für Hamster Plus, IV, Pro 20, Pro).
- Anleitung „Fingerabdruckscanner einrichten“ in Einrichtung und Wartungsmenü: Schritte, unterstützte
  Scanner, was tun, wenn es nicht klappt. Fehlt ein Treiber, ist „Geräte einrichten“ der hervorgehobene
  nächste Schritt und die Anleitung aufgeklappt. Ausführlich: `docs/GERAETE-WINDOWS.md`.

## 0.4.1 – 2026-10-08

- „Geräte einrichten“ installiert die signoPAD-API für das Unterschriftenpad selbst: Die App lädt das
  offizielle Setup von signotec (9.0.1, 64 Bit), prüft Größe und SHA-256 und installiert still Treiber und
  Bibliothek (eine Rückfrage der Benutzerkontensteuerung). Für andere Dateien gibt es keinen Download.
- Geräteliste in Einrichtung und Wartung: Fehlt etwas, steht darunter, was zu tun ist – mit „Download-Seite
  öffnen“ als Rückfall (nur freigegebene Seiten der Hersteller).

## 0.4.0 – 2026-10-08

- signotec-Unterschriftenpad per USB am Platz (Sigma, Zeta, Omega, Gamma, Delta, Alpha): Die App lädt die
  `STPadLib.dll` der signoPAD-API (64 Bit) von signotec – aus dem Gerätepaket-Ordner oder aus dem
  Programmordner, in den das Setup von signotec sie legt – und macht, was am Studio-Server der
  Hilfsprozess tut: Bildschirm vom Server zeigen, Stiftpunkte gebündelt und verschlüsselt melden
  (Spiegelung am Bildschirm), Tasten Löschen/OK/Abbrechen am Pad wie am Bildschirm, am Ende Bild und
  SignData. Gerendert, durchsichtig gemacht und am Vertrag abgelegt wird weiter am Server.
- Ohne signoPAD-API meldet die Geräteliste das Pad mit „Treiber fehlt“ und dem Hinweis, sie zu installieren.

## 0.3.0 – 2026-10-07

Stufe 2 der Kassenplätze, zweiter Teil. Braucht den Studio-Server mit Bondrucker am Platz; an einem
älteren Server bleibt alles wie in 0.2.0.

- Bondrucker per USB am Platz: Der Server schickt den fertigen Bon (ESC/POS) als Auftrag „drucken“,
  die App gibt ihn roh über den Windows-Druckspooler aus. Welcher Drucker, stellt der Server ein;
  sonst nimmt die App den ersten, der nach Bondrucker aussieht (Epson, Star, TM-, Bon …), sonst den
  Standarddrucker. Der Herzschlag meldet die Drucker in Windows zur Auswahl.
- QR-/Barcode-Scanner im Tastaturmodus gelten als „bereit“: Sie brauchen die App nicht, die
  Oberfläche erkennt ihren Scan selbst.

## 0.2.0 – 2026-10-07

Stufe 2 der Kassenplätze, erster Teil. Braucht den Studio-Server mit Kassenplätze Stufe 2
(Zoom und `ich/ereignis`); an einem älteren Server bleibt alles wie in 0.1.0.

- Zoom je Platz: Die Größe der Oberfläche stellt der Platz am Server ein (50–200 %). Die App
  übernimmt sie beim nächsten Herzschlag ohne Neuladen; eigene Seiten bleiben bei 100 %.
- Magnetkartenleser am Platz (USB-Seriell-Wandler CP210x, FTDI, PL2303, CH340 – unter Windows ein
  COM-Anschluss): Ein Thread hält den Anschluss offen, stößt den Leser über DTR/RTS an und meldet
  jede Karte verschlüsselt an den Server. Kartennummern kommen in kein Protokoll.
  Wartungsmenü und Geräteliste zeigen den Leser mit Anschluss und Zustand.

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
