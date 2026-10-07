; Studio Kassenplatz – Ergänzungen zum NSIS-Installer
;
; Die App findet den Studio-Server per mDNS (UDP 5353). Damit die Windows-Firewall nicht
; im laufenden Kiosk nachfragt, wird die Regel beim Installieren angelegt. Pro-Benutzer-
; Installationen haben dafür keine Rechte – dann fragt Windows einmal bei der Einrichtung,
; während jemand am Platz ist (docs/KIOSK-WINDOWS.md).

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec 'netsh advfirewall firewall add rule name="Studio Kassenplatz (mDNS)" dir=in action=allow protocol=UDP localport=5353 program="$INSTDIR\${MAINBINARYNAME}.exe" profile=private'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  nsExec::Exec 'netsh advfirewall firewall delete rule name="Studio Kassenplatz (mDNS)"'
!macroend
