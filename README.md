# Studio Kassenplatz

Windows-App für weitere Plätze im Studio: die **Theke**, einen **Self-Service-Kiosk** zum
Einbuchen und Bezahlen ohne Personal und eine **Verfügbarkeitsanzeige** der freien Kabinen.
Die App ist eine schlanke Hülle um die Oberfläche des Studio-Servers. Sie koppelt den PC, öffnet
die Oberfläche im Vollbild und bindet Geräte ein, die per USB am PC stecken.

Eingestellt wird jeder Platz am Studio-Server unter **Kassenverwaltung → Kassenplätze**
(App „Kassenplätze“): Art, Angebot, Zahlarten, Aussehen und Geräte.

## Installieren

1. Unter [Releases](https://github.com/boeckmannniklas-web/studio-tauri/releases/latest) die Datei
   `Studio Kassenplatz_…_x64-setup.exe` laden und am PC ausführen. Windows 10/11 (64 Bit).
2. Am Studio-Server unter Kassenverwaltung → Kassenplätze einen Platz anlegen. Es erscheint ein
   Koppelcode (`ABC DEF`, 15 Minuten gültig) mit QR-Code.
3. Die App sucht den Studio-Server selbst (`studio.local`). Code eingeben oder den QR-Code mit
   einem Scanner ins Feld scannen → **Koppeln**.
4. Steckt ein Fingerabdruckscanner am PC: **Geräte einrichten** installiert Treiber und
   Herstellerbibliothek (fragt nach Administratorrechten). Ein Magnetkartenleser am
   USB-Seriell-Wandler (etwa CP2102) braucht nur den Windows-Treiber des Wandlers, den Windows
   meist selbst holt; die App findet seinen COM-Anschluss von allein.
5. **Starten**. Ab jetzt startet die App mit Windows und öffnet die Oberfläche von selbst.

Die App ist pro Benutzer installiert: Updates laufen ohne Administratorrechte, nachts zwischen
3 und 4 Uhr oder sofort über das Wartungsmenü.

## Wartung

**Strg + Alt + S** öffnet das Wartungsmenü. Es verlangt die Wartungs-PIN des Platzes (festgelegt
in der Kassenverwaltung). Darin: Adresse des Servers ändern, Geräte einrichten, Update
installieren, entkoppeln, App beenden. Anders lässt sich die App nicht schließen.

Einen öffentlich zugänglichen Kiosk zusätzlich auf Windows-Ebene absichern:
[docs/KIOSK-WINDOWS.md](docs/KIOSK-WINDOWS.md).

## Sicherheit

* Die Oberfläche des Studio-Servers läuft im Fenster der App, bekommt aber **keinen Zugriff auf
  die App**: Die Befehle der App stehen nur ihren eigenen Seiten offen (Capability `lokal`,
  App-Manifest in `build.rs`, Tauri ≥ 2.11.1). Alles Native – Geräte, Treiber, Updates – läuft
  zwischen dem Agent der App und dem Server.
* Navigiert wird nur zum gekoppelten Server; Links nach draußen bleiben zu.
* Gerätetoken und Bildschlüssel liegen in der Windows-Anmeldeinformationsverwaltung.
* Fingerabdruck: Der Platz schickt nur das Bild, verschlüsselt (AES-256-GCM, Schlüssel aus der
  Kopplung). Merkmale und Vergleich rechnet der Server; gespeichert wird hier nichts.
* Magnetkarte: Jede Karte geht verschlüsselt an den Server (derselbe Schlüssel, die Kennung der
  Meldung als Zusatzangabe); wem sie gehört, weiß nur der Server. Die Nummer kommt in kein Protokoll.
* Updates sind signiert (minisign); der öffentliche Schlüssel steht in `src-tauri/tauri.conf.json`.
* Herstellerdateien (SecuGen) liegen nie in diesem Repo. Der Server liefert sie als Gerätepaket.
* Im Studio-Netz spricht die App HTTP mit dem Server (wie die Browser bisher). TLS im LAN folgt.

## Größe der Oberfläche

Der Zoom des Platzes (50–200 %) wird am Server eingestellt (Kassenverwaltung → Kassenplätze →
Allgemein) und gilt nach dem nächsten Herzschlag. Strg + / − bleiben am Platz gesperrt.

## Entwickeln

```
npm ci
cd src-tauri && cargo test --lib
npx tauri dev       # unter Windows
```

Release: Version in `package.json`, `src-tauri/Cargo.toml` und `src-tauri/tauri.conf.json` gleich
setzen, `CHANGELOG.md` ergänzen, Tag `v<version>` pushen. Der Workflow baut den Installer, signiert
das Update (Secrets `TAURI_SIGNING_PRIVATE_KEY`/`…_PASSWORD`) und legt `latest.json` an den Release.
