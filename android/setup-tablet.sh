#!/usr/bin/env bash
# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Anmeldung übersprungen, WLAN verbunden,
# ADB-Debugging an, USB angeschlossen, adb im PATH.
set -euo pipefail
APK="${1:-bts-light-tablet.apk}"
PAKET="de.badhub.btslight.tablet"

echo "Geräte:"; adb devices
n=$(adb devices | grep -c 'device$' || true)
[ "$n" -eq 1 ] || { echo "Genau ein Tablet per USB anschließen (gefunden: $n)." >&2; exit 1; }

if adb shell dumpsys account | grep -q 'Account {'; then
  echo "Auf dem Tablet ist noch ein Konto eingerichtet (Amazon?). Zurücksetzen, Anmeldung überspringen, erneut starten." >&2
  exit 1
fi

echo "APK installieren: $APK"; adb install -r "$APK"
echo "Gerätebesitzer setzen"; adb shell dpm set-device-owner "$PAKET/.kiosk.KioskAdminReceiver"
echo "WebView-Stand:"
for p in com.amazon.webview.chromium com.google.android.webview com.android.webview; do
  adb shell dumpsys package "$p" 2>/dev/null | grep versionName | sed "s/^/  $p /" || true
done
echo "App starten"; adb shell am start -n "$PAKET/.KioskActivity"
echo "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."