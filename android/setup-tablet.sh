#!/usr/bin/env bash
# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Anmeldung übersprungen, WLAN verbunden (oder
# WLAN_SSID/WLAN_PASSWORT gesetzt), ADB-Debugging an, USB angeschlossen, adb im PATH.
set -euo pipefail
APK="${1:-bts-light-tablet.apk}"
# Zweites Argument "behalten": Amazon-Apps (und den OTA-Dienst) nicht entfernen.
ENTSCHLACKEN="${2:-}"
# Hallen-WLAN gleich mit einrichten (WPA2): WLAN_SSID=… WLAN_PASSWORT=… ./setup-tablet.sh
WLAN_SSID="${WLAN_SSID:-}"
WLAN_PASSWORT="${WLAN_PASSWORT:-}"
PAKET="de.badhub.btslight.tablet"

# WLAN per adb verbinden (`cmd wifi connect-network`, ab Android 11) und auf
# eine IPv4-Adresse warten. Scheitert nur weich: Ohne WLAN geht die
# Einrichtung weiter, die App sucht den Turnier-PC dann später.
wlan_einrichten() {
  local r i ip
  adb shell cmd wifi set-wifi-enabled enabled >/dev/null 2>&1 || true
  r=$(adb shell cmd wifi connect-network "$WLAN_SSID" wpa2 "$WLAN_PASSWORT" 2>&1 | tr -d '\r' || true)
  case "$r" in
    *"Connection initiated"*) ;;
    *) echo "WLAN-Befehl nicht angenommen: $r – WLAN bitte von Hand verbinden." >&2; return 0 ;;
  esac
  for i in $(seq 1 30); do
    sleep 1
    ip=$(adb shell ip -4 addr show wlan0 2>/dev/null | tr -d '\r' | sed -n 's/.*inet \([0-9.]*\).*/\1/p' | head -1)
    if [ -n "$ip" ]; then echo "  WLAN $WLAN_SSID verbunden, IP $ip"; return 0; fi
  done
  echo "WLAN $WLAN_SSID: nach 30 s keine IP-Adresse – SSID/Passwort prüfen, Einrichtung läuft weiter." >&2
}

# Amazon-Apps, die auf einem Zähl-Tablet nur Akku und Hintergrund kosten —
# dieselbe Liste wie in setup-tablet.ps1 und kern/Entschlackung.kt (dort
# pflegen!). Deaktiviert für Nutzer 0 (`pm disable-user --user 0`; `pm
# uninstall -k` verweigert Fire OS 8 sogar für die Wetter-App), zurück mit
# `pm enable --user 0 <paket>`. „Protected" Pakete (OTA, Sonderangebote)
# meldet das Skript als verweigert — die versteckt die App selbst als
# Gerätebesitzer beim ersten Start.
AMAZON_APPS="
com.amazon.dee.app
com.amazon.dee.alexaonandroidos
com.amazon.weather
com.amazon.photos
com.amazon.avod
com.amazon.mp3
com.audible.application.kindle
com.amazon.kindle
com.amazon.kindle.kso
com.amazon.windowshop
com.amazon.tahoe
com.amazon.hedwig
com.amazon.imdb.tv.mobile.app
com.amazon.cloud9.kids
com.amazon.device.software.ota
com.amazon.device.software.ota.override
com.amazon.kindle.otter.oobe.forced.ota
"

entschlacken() {
  local installiert weg=0 verweigert=0 fehlt=0 p r
  installiert=$(adb shell pm list packages --user 0 2>/dev/null | tr -d '\r' | sed 's/^package://')
  for p in $AMAZON_APPS; do
    # Here-String statt Pipe: `grep -q` bricht beim Treffer ab und würde
    # `printf` unter pipefail in SIGPIPE schicken (siehe Konten-Prüfung oben).
    if ! grep -qxF -- "$p" <<<"$installiert"; then fehlt=$((fehlt+1)); continue; fi
    r=$(adb shell pm disable-user --user 0 "$p" 2>&1 | tr -d '\r' || true)
    case "$r" in
      *"new state: disabled"*) echo "  deaktiviert $p"; weg=$((weg+1)) ;;
      *) echo "  verweigert  $p (geschützt – die App versteckt es als Gerätebesitzer)"; verweigert=$((verweigert+1)) ;;
    esac
  done
  echo "Entschlackt: $weg deaktiviert, $verweigert verweigert, $fehlt nicht vorhanden."
}

echo "Geräte:"; adb devices
n=$(adb devices | grep -c 'device$' || true)
[ "$n" -eq 1 ] || { echo "Genau ein Tablet per USB anschließen (gefunden: $n)." >&2; exit 1; }

if [ -n "$WLAN_SSID" ]; then echo "WLAN einrichten: $WLAN_SSID"; wlan_einrichten; fi

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
if [ "$ENTSCHLACKEN" = "behalten" ]; then echo "Amazon-Apps bleiben (behalten)."
else echo "Amazon-Apps entfernen (Alexa, Video, Kindle, OTA …)"; entschlacken; fi
echo "WebView-Stand:"
for p in com.amazon.webview.chromium com.google.android.webview com.android.webview; do
  adb shell dumpsys package "$p" 2>/dev/null | grep versionName | sed "s/^/  $p /" || true
done
echo "App starten"; adb shell am start -n "$PAKET/.KioskActivity"
echo "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
