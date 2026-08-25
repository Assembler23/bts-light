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

; Der verschluesselte Port (TCP 8443, ADR 0047) bekommt eine EIGENE Regel
; statt die bestehende um einen Port zu erweitern. Grund: Ein "add rule" mit
; schon vorhandenem Namen legt in netsh eine ZWEITE, gleichnamige Regel an,
; und ein sauberes "delete + add" braeuchte einen zweiten Prozess - also
; entweder eine doppelte UAC-Abfrage oder einen cmd.exe-Umweg mit heiklem
; Quoting. Eine getrennte Regel laesst die Bestandsregel unberuehrt und ist
; fuer sich genommen offensichtlich richtig. Preis: zwei UAC-Abfragen beim
; interaktiven Setup.

!macro NSIS_HOOK_POSTINSTALL
  IfSilent btslight_fw_add_done
  DetailPrint "Firewall-Regel fuer den Tablet-Server (Port 8088) wird angelegt ..."
  ExecShellWait "runas" "netsh.exe" 'advfirewall firewall add rule name="BTS Light (Tablets)" dir=in action=allow protocol=TCP localport=8088 enable=yes profile=any' SW_HIDE
  DetailPrint "Firewall-Regel fuer den verschluesselten Zugang (Ports 443 und 8443) wird angelegt ..."
  ExecShellWait "runas" "netsh.exe" 'advfirewall firewall add rule name="BTS Light (Tablets, verschluesselt)" dir=in action=allow protocol=TCP localport=443,8443 enable=yes profile=any' SW_HIDE
  btslight_fw_add_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  IfSilent btslight_fw_del_done
  ExecShellWait "runas" "netsh.exe" 'advfirewall firewall delete rule name="BTS Light (Tablets)"' SW_HIDE
  ExecShellWait "runas" "netsh.exe" 'advfirewall firewall delete rule name="BTS Light (Tablets, verschluesselt)"' SW_HIDE
  btslight_fw_del_done:
!macroend
