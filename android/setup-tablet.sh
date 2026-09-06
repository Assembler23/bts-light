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

# Device Owner geht nur ohne eingerichtete Konten. `grep -q` bricht bei einem
# Treffer die Pipe früh ab und schickt `adb` unter `set -o pipefail` in
# SIGPIPE — mit einer Variable zwischenspeichern statt direkt zu pipen.
konten=$(adb shell dumpsys account 2>/dev/null || true)
case "$konten" in
  *'Account {'*)
    echo "Auf dem Tablet ist noch ein Konto eingerichtet (Amazon?). Zurücksetzen, Anmeldung überspringen, erneut starten." >&2
    exit 1
    ;;
esac

echo "APK installieren: $APK"
if ! adb install -r "$APK"; then
  echo "adb install fehlgeschlagen." >&2
  exit 1
fi

echo "Gerätebesitzer setzen"
if ! adb shell dpm set-device-owner "$PAKET/.kiosk.KioskAdminReceiver"; then
  echo "Gerätebesitzer konnte nicht gesetzt werden. Meist: noch ein Konto auf dem Tablet, oder schon ein anderer Gerätebesitzer." >&2
  exit 1
fi
echo "WebView-Stand:"
for p in com.amazon.webview.chromium com.google.android.webview com.android.webview; do
  adb shell dumpsys package "$p" 2>/dev/null | grep versionName | sed "s/^/  $p /" || true
done
echo "App starten"; adb shell am start -n "$PAKET/.KioskActivity"
echo "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
