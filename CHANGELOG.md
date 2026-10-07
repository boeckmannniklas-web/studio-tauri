# Changelog

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
