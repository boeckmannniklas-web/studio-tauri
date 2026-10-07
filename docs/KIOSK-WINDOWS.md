# Self-Service-Kiosk unter Windows absichern

Die App sperrt sich selbst: Vollbild, immer im Vordergrund, Schließen nur über das Wartungsmenü,
Kontextmenü und Entwicklerwerkzeuge aus. Strg+Alt+Entf, die Windows-Taste oder ein Abmelden kann
eine App aber nicht verhindern. Für einen öffentlich zugänglichen Kiosk deshalb zusätzlich:

## 1. Eigenes Windows-Konto für den Kiosk

1. Lokales Standardkonto anlegen (kein Administrator), z. B. `kiosk`.
2. Als `kiosk` anmelden und die App **in diesem Konto** installieren und koppeln (die App ist pro
   Benutzer installiert). Treiber verlangen dabei einmal Administratorrechte – die gibt ein
   Techniker in der Rückfrage ein.
3. Automatische Anmeldung für `kiosk` einrichten (`netplwiz` bzw. Sysinternals *Autologon*).

## 2. Die App als Oberfläche statt des Explorers (Shell-Ersatz)

Für das Konto `kiosk` startet Windows dann nur die App – ohne Startmenü und Taskleiste.

Als Administrator in einer Eingabeaufforderung (mit der SID des Kontos `kiosk`, `wmic useraccount
where name='kiosk' get sid` oder `whoami /user` als `kiosk`):

```
reg load HKU\Kiosk C:\Users\kiosk\NTUSER.DAT
reg add "HKU\Kiosk\Software\Microsoft\Windows NT\CurrentVersion\Winlogon" /v Shell /t REG_SZ /d "\"C:\Users\kiosk\AppData\Local\Studio Kassenplatz\studio-kassenplatz.exe\"" /f
reg unload HKU\Kiosk
```

(Pfad der Programmdatei nach der Installation prüfen.) Zurück zum normalen Desktop: den Wert
`Shell` wieder löschen.

Unter Windows 10/11 **Enterprise/Education/IoT** geht dasselbe sauberer mit dem
*Shell Launcher* bzw. der zugewiesenen Zuweisung (Assigned Access) in den Einstellungen.

## 3. Weitere Einstellungen

* Energie: Bildschirm nie ausschalten, kein Standby.
* Windows Update: Neustarts in die Nacht legen (Nutzungszeit).
* Benachrichtigungen aus (Fokus-Assistent).
* Touch: Wischgesten vom Rand aus (`EdgeUI`, Gruppenrichtlinie „Edge-Wischen erlauben“ = deaktiviert).
* Die Wartungs-PIN des Platzes in der Kassenverwaltung setzen – ohne sie lässt sich die App am Platz
  nicht beenden und nichts umstellen.
