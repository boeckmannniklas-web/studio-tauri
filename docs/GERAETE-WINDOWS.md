# Geräte am Kassenplatz einrichten (Windows)

Für alle, die einen Kassenplatz aufstellen: Theke, Self-Service-Kiosk oder Verfügbarkeitsanzeige.

## Fingerabdruckscanner (SecuGen)

**Unterstützt** (Windows 10/11, 64 Bit):

| Scanner | Treiber im Gerätepaket | USB-ID (VID 1162) |
|---|---|---|
| Hamster Plus – FDU03, SDU03M, SDU03P | FDU03 2.9.0.1 | 0320, 0322, 0325, 1000, 3000 |
| Hamster IV – FDU04A, SDU04P | FDU04 2.6.0.1 | 0330, 2000, 4000 |
| Hamster Pro 20 – U20 | HU20 1.4.9.1 | 2200 |
| Hamster Pro / Pro Duo – UPx, UPx-P | HUPx 1.3.2.1 | 2201, 2301 |

Dazu die SecuGen-Bibliothek aus dem *FDx SDK Pro for Windows* 4.3.1 (`sgfplib.dll` mit
`sgfpamx.dll`, `sgwsqlib.dll`, `sgfdusdax64.dll`, `sgbledev.dll`).

### Mit einem Klick

1. Studio Kassenplatz installieren und mit dem Studio-Server koppeln.
2. Scanner direkt am PC einstecken (kein USB-Hub). Er erscheint unter „Geräte an diesem PC“ mit
   **„Treiber fehlt“**.
3. **„Geräte einrichten“** tippen – bei der Einrichtung oder später im Wartungsmenü (Strg+Alt+S, PIN).
4. Die Frage der Benutzerkontensteuerung mit **Ja** beantworten (einmal). Ohne Administratorkonto:
   Anmeldung eines Administrators eingeben.
5. Der Scanner steht auf **„bereit“**. Kein Neustart nötig.

Was dabei passiert: Die App holt das Gerätepaket vom Studio-Server, prüft jede Datei per SHA-256,
legt sie nach `C:\ProgramData\StudioKassenplatz\geraete\secugen` und installiert die signierten
Treiber mit `pnputil`. Die Herstellerdateien liegen nicht in diesem öffentlichen Repo; der
Studio-Server bringt sie mit.

### Wenn es nicht klappt

| Meldung / Verhalten | Abhilfe |
|---|---|
| „Für „secugen“ liegt kein Gerätepaket auf diesem Server“ | Studio-Server aktualisieren (das Paket kommt mit dem Server-Update). |
| Scanner bleibt auf „Treiber fehlt“ | App im Wartungsmenü beenden, neu starten. |
| Scanner taucht nicht auf | Anderen USB-Anschluss/anderes Kabel. Prüfen: `powershell -NoProfile -Command "Get-PnpDevice -PresentOnly \| Where-Object InstanceId -like '*VID_1162*'"` – Status muss `OK` sein. |
| „SecuGen-Bibliothek nicht ladbar“ | 32-Bit-DLLs erwischt – es müssen die aus `bin\x64` sein. |
| Aufnahme bricht mit „Kein Finger aufgelegt“ ab | Sensor reinigen, Finger flach auflegen; hilft das nicht, ist der Berührungssensor des Scanners defekt. |

### Von Hand (Rückfall)

In einer **Eingabeaufforderung als Administrator**:

```
pnputil /add-driver "C:\SecuGen\FDU03\SGFu03x64.inf" /install
mkdir "C:\ProgramData\StudioKassenplatz\geraete\secugen"
copy "<SDK>\FDx SDK Pro for Windows v4.3.1\bin\x64\*.dll" "C:\ProgramData\StudioKassenplatz\geraete\secugen\"
```

Treiber gibt es bei SecuGen unter „Drivers“, das SDK über das Anfrageformular
(https://secugen.com/). Danach die App neu starten.

## Unterschriftenpad (signotec)

„Geräte einrichten“ lädt die signoPAD-API 9.0.1 (64 Bit) direkt von signotec, prüft Größe und
Prüfsumme und installiert sie still (eine Rückfrage). Rückfall: „Download-Seite öffnen“ in der
Geräteliste.

## Ohne Einrichtung

Magnetkartenleser (USB-Seriell), QR-/Barcode-Scanner im Tastaturmodus und Bondrucker (über den
Windows-Drucker) brauchen keine Treiber aus der App.
