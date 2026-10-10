; Studio Kassenplatz – Ergänzungen zum NSIS-Installer
;
; Die App findet den Studio-Server per mDNS (UDP 5353). Damit die Windows-Firewall nicht
; im laufenden Kiosk nachfragt, wird die Regel beim Installieren angelegt. Pro-Benutzer-
; Installationen haben dafür keine Rechte – dann fragt Windows einmal bei der Einrichtung,
; während jemand am Platz ist (docs/KIOSK-WINDOWS.md).

;
; Mit Windows starten? Gefragt wird einmal, bei der ersten Installation von Hand. Updates (still oder
; passiv über den Updater) fragen nie – ein Kiosk darf nicht an einer Rückfrage hängen bleiben. Die
; Antwort steht unter HKCU\Software\Studio Kassenplatz, Wert Autostart (1/0); die App liest sie beim
; Start. Gibt die Kassenverwaltung für den Platz Ja oder Nein vor, gilt das (App ab 0.5.0).

!define STUDIO_REG "Software\Studio Kassenplatz"

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec 'netsh advfirewall firewall add rule name="Studio Kassenplatz (mDNS)" dir=in action=allow protocol=UDP localport=5353 program="$INSTDIR\${MAINBINARYNAME}.exe" profile=private'

  ClearErrors
  ReadRegDWORD $R9 HKCU "${STUDIO_REG}" "Autostart"
  ${If} ${Errors}
    ${If} $PassiveMode = 1
    ${OrIf} $UpdateMode = 1
    ${OrIf} ${Silent}
      ; ohne Rückfrage: wie bisher mit Windows starten
      WriteRegDWORD HKCU "${STUDIO_REG}" "Autostart" 1
    ${Else}
      MessageBox MB_YESNO|MB_ICONQUESTION "Soll der Kassenplatz automatisch mit Windows starten?$\r$\n$\r$\nFür Self-Service-Kiosk und Verfügbarkeitsanzeige empfohlen – dann läuft der Platz nach einem Neustart oder Stromausfall von selbst wieder.$\r$\n$\r$\nÄndern lässt es sich später in der Kassenverwaltung beim Platz." /SD IDYES IDNO studio_autostart_nein
      WriteRegDWORD HKCU "${STUDIO_REG}" "Autostart" 1
      Goto studio_autostart_fertig
      studio_autostart_nein:
      WriteRegDWORD HKCU "${STUDIO_REG}" "Autostart" 0
      studio_autostart_fertig:
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="Studio Kassenplatz (mDNS)"'
  ; Bei einem Update deinstalliert der Installer nicht – nur ein echtes Entfernen vergisst die Wahl
  DeleteRegKey HKCU "${STUDIO_REG}"
!macroend
