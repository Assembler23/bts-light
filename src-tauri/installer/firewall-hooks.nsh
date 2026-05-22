; ===========================================================================
; NSIS-Installer-Hooks fuer BTS Light.
;
; Legt beim interaktiven Setup eine eingehende Windows-Firewall-Regel fuer
; den Tablet-Server (TCP 8088) an. Damit entfaellt die "Zugriff zulassen?"-
; Abfrage, die sonst beim ersten Start in der Halle aufpoppt - und die ohne
; Admin-Rechte gar nicht bestaetigt werden kann.
;
; Nur beim interaktiven Setup (IfSilent-Guard): Das stille Auto-Update
; fuehrt denselben Installer aus - ohne den Guard kaeme bei jedem Update
; eine UAC-Abfrage. Die Regel ueberlebt Updates ohnehin.
;
; netsh advfirewall braucht Admin-Rechte; der Installer laeuft per-user
; (nicht erhoeht), daher per ExecShellWait "runas" - das zeigt einmalig
; eine UAC-Abfrage. Lehnt der Nutzer ab, wird die Regel nicht angelegt
; (kein Abbruch) - dann erscheint spaeter die normale Firewall-Abfrage.
; ===========================================================================

!macro NSIS_HOOK_POSTINSTALL
  IfSilent btslight_fw_add_done
  DetailPrint "Firewall-Regel fuer den Tablet-Server (Port 8088) wird angelegt ..."
  ExecShellWait "runas" "netsh.exe" 'advfirewall firewall add rule name="BTS Light (Tablets)" dir=in action=allow protocol=TCP localport=8088 enable=yes profile=any' SW_HIDE
  btslight_fw_add_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  IfSilent btslight_fw_del_done
  ExecShellWait "runas" "netsh.exe" 'advfirewall firewall delete rule name="BTS Light (Tablets)"' SW_HIDE
  btslight_fw_del_done:
!macroend
