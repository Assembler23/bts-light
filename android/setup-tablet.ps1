# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Amazon-/Google-Anmeldung übersprungen,
# WLAN verbunden, ADB-Debugging an, USB angeschlossen, adb im PATH.
param([string]$Apk = "bts-light-tablet.apk")

$ErrorActionPreference = "Stop"
$Paket = "de.badhub.btslight.tablet"

Write-Host "Geräte:"; adb devices
$geraete = @(adb devices | Select-String "device$").Count
if ($geraete -ne 1) { Write-Error "Genau ein Tablet per USB anschließen (gefunden: $geraete)."; exit 1 }

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

Write-Host "WebView-Stand (für die Doku/Fehlersuche):"
foreach ($p in "com.amazon.webview.chromium", "com.google.android.webview", "com.android.webview") {
  $v = adb shell dumpsys package $p | Select-String "versionName"
  if ($v) { Write-Host "  $p $v" }
}

Write-Host "App starten"
adb shell am start -n "$Paket/.KioskActivity"
Write-Host "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
