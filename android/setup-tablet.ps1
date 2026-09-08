# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Amazon-/Google-Anmeldung übersprungen,
# WLAN verbunden (oder -Wlan/-WlanPasswort angeben), ADB-Debugging an, USB angeschlossen, adb im PATH.
param(
  [string]$Apk = "bts-light-tablet.apk",
  # Hallen-WLAN gleich mit einrichten (WPA2; leer = WLAN von Hand verbinden).
  [string]$Wlan = "",
  [string]$WlanPasswort = "",
  # Amazon-Apps behalten (Alexa, Video, Kindle … und den OTA-Dienst).
  [switch]$OhneEntschlacken
)

$ErrorActionPreference = "Stop"
$Paket = "de.badhub.btslight.tablet"

# WLAN per adb verbinden (`cmd wifi connect-network`, ab Android 11) und auf
# eine IPv4-Adresse warten. Scheitert nur weich: Ohne WLAN geht die
# Einrichtung weiter, die App sucht den Turnier-PC dann später.
function WlanEinrichten([string]$Ssid, [string]$Passwort) {
  $ErrorActionPreference = "Continue"
  adb shell cmd wifi set-wifi-enabled enabled | Out-Null
  $r = (adb shell cmd wifi connect-network "$Ssid" wpa2 "$Passwort" 2>&1 | Out-String)
  if ($r -notmatch "Connection initiated") { Write-Warning "WLAN-Befehl nicht angenommen: $($r.Trim()) – WLAN bitte von Hand verbinden."; return }
  for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 1
    $ip = (adb shell ip -4 addr show wlan0 2>$null | Select-String "inet (\d+\.\d+\.\d+\.\d+)").Matches
    if ($ip.Count -gt 0) { Write-Host "  WLAN $Ssid verbunden, IP $($ip[0].Groups[1].Value)"; return }
  }
  Write-Warning "WLAN ${Ssid}: nach 30 s keine IP-Adresse – SSID/Passwort prüfen, Einrichtung läuft weiter."
}

# Amazon-Apps, die auf einem Zähl-Tablet nur Akku und Hintergrund kosten.
# Deaktiviert für Nutzer 0 (`pm disable-user --user 0`; `pm uninstall -k`
# verweigert Fire OS 8 selbst für die Wetter-App). Zurück mit
# `pm enable --user 0 <paket>`. Pakete, die Fire OS als „protected" auch
# gegen disable-user schützt (OTA, Sonderangebote), meldet das Skript als
# verweigert — die versteckt die App selbst als Gerätebesitzer beim ersten
# Start (`kern/Entschlackung.kt`, dieselbe Liste; dort pflegen!).
$AmazonApps = @(
  # Akku-/Hintergrundfresser
  "com.amazon.dee.app",                 # Alexa
  "com.amazon.dee.alexaonandroidos",    # Alexa-Dienst
  "com.amazon.weather",                 # Wetter
  "com.amazon.photos",                  # Amazon Photos (Sync)
  "com.amazon.avod",                    # Prime Video
  "com.amazon.mp3",                     # Amazon Music
  "com.audible.application.kindle",     # Audible
  "com.amazon.kindle",                  # Kindle
  "com.amazon.kindle.kso",              # Sonderangebote (Sperrbildschirm-Werbung)
  "com.amazon.windowshop",              # Amazon Shopping
  "com.amazon.tahoe",                   # Amazon Kids
  "com.amazon.hedwig",                  # Hilfe
  "com.amazon.imdb.tv.mobile.app",      # Freevee/IMDb TV
  "com.amazon.cloud9.kids",             # Silk Kids
  # Amazon-Systemupdates: kein Fire-OS-Update mitten im Turnier, keine
  # überschriebene Einrichtung. Vorher einmal von Hand aktualisieren!
  "com.amazon.device.software.ota",
  "com.amazon.device.software.ota.override",
  "com.amazon.kindle.otter.oobe.forced.ota"
)

# Deaktiviert die Amazon-Apps für Nutzer 0; je Paket eine Zeile, Verweigerung
# bricht nicht ab. Achtung PowerShell 5.1: `2>&1` verpackt jede stderr-Zeile
# eines nativen Programms in einen ErrorRecord, und unter "Stop" wäre die
# SecurityException von `pm` dann ein Skript-Abbruch (Review-Befund). Darum
# in dieser Funktion "Continue" — gilt nur hier (Funktions-Scope).
function Entschlacken {
  $ErrorActionPreference = "Continue"
  $installiert = (adb shell pm list packages --user 0) -replace "package:", "" -replace "\r", ""
  $weg = 0; $verweigert = 0; $fehlt = 0
  foreach ($p in $AmazonApps) {
    if ($installiert -notcontains $p) { $fehlt++; continue }
    $r = (adb shell pm disable-user --user 0 $p 2>&1 | Out-String)
    if ($r -match "new state: disabled") { Write-Host "  deaktiviert $p"; $weg++ }
    else { Write-Host "  verweigert  $p (geschützt – die App versteckt es als Gerätebesitzer)"; $verweigert++ }
  }
  Write-Host "Entschlackt: $weg deaktiviert, $verweigert verweigert, $fehlt nicht vorhanden."
}

Write-Host "Geräte:"; adb devices
$geraete = @(adb devices | Select-String "device$").Count
if ($geraete -ne 1) { Write-Error "Genau ein Tablet per USB anschließen (gefunden: $geraete)."; exit 1 }

if ($Wlan) { Write-Host "WLAN einrichten: $Wlan"; WlanEinrichten $Wlan $WlanPasswort }

# Device Owner geht nur ohne eingerichtete Konten.
$konten = adb shell dumpsys account | Select-String "Account \{"
if ($konten) {
  Write-Error "Auf dem Tablet ist noch ein Konto eingerichtet (Amazon?). Tablet zurücksetzen, Anmeldung überspringen, erneut starten."
  exit 1
}

Write-Host "APK installieren: $Apk"
# $ErrorActionPreference greift bei nativen Programmen (adb) nicht – Exit-Code selbst prüfen.
adb install -r $Apk
if ($LASTEXITCODE -ne 0) { Write-Error "adb install fehlgeschlagen (Exit $LASTEXITCODE)."; exit 1 }

Write-Host "Gerätebesitzer setzen"
adb shell dpm set-device-owner "$Paket/.kiosk.KioskAdminReceiver"
if ($LASTEXITCODE -ne 0) { Write-Error "Gerätebesitzer konnte nicht gesetzt werden (Exit $LASTEXITCODE). Meist: noch ein Konto auf dem Tablet, oder schon ein anderer Gerätebesitzer."; exit 1 }

if ($OhneEntschlacken) { Write-Host "Amazon-Apps bleiben (-OhneEntschlacken)." }
else { Write-Host "Amazon-Apps entfernen (Alexa, Video, Kindle, OTA …)"; Entschlacken }

Write-Host "WebView-Stand (für die Doku/Fehlersuche):"
foreach ($p in "com.amazon.webview.chromium", "com.google.android.webview", "com.android.webview") {
  $v = adb shell dumpsys package $p | Select-String "versionName"
  if ($v) { Write-Host "  $p $v" }
}

Write-Host "App starten"
adb shell am start -n "$Paket/.KioskActivity"
Write-Host "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
