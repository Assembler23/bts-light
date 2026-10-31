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

# Amazon-/Fremdpakete, die auf einem Zähl-Tablet nur Akku, Hintergrund und
# Netz kosten — Grundlage: Debloat-Liste von Fire-Tools. Dieselbe Liste wie
# in setup-tablet.ps1 und kern/Entschlackung.kt (dort pflegen, die Skripte
# erzeugt/prüft scripts/test-entschlackung-liste.mjs). Bewusst NICHT dabei:
# WebView, Silk, Appstore, Launcher, Kindersicherung, dcp/imp, Fire-Tastatur
# (redstone — sonst keine PIN-Eingabe), Einstellungen.
AMAZON_APPS="
amazon.speech.sim
com.amazon.afe.app
com.amazon.alexa.multimodal.gemini
com.amazon.alexa.youtube.app
com.amazon.cardinal
com.amazon.comms.kids
com.amazon.dee.alexaonandroidos
com.amazon.dee.app
com.amazon.smartgenie
com.amazon.tablet.voiceassistant
com.amazon.avod
com.amazon.bioscope
com.amazon.tv.launcher
com.amazon.tv.ottssocompanionapp
com.amazon.imdb.tv.mobile.app
com.amazon.mp3
com.audible.application.kindle
com.amazon.kindle
com.amazon.webapp
com.goodreads.kindle
com.amazon.kindle.personal_video
com.amazon.photos
com.amazon.photos.importer
com.amazon.windowshop
com.amazon.iris
com.amazon.weather
com.amazon.zico
com.kingsoft.office.amz
com.amazon.tahoe
com.amazon.cloud9.kids
com.amazon.ags.app
com.amazon.geo.client.maps
com.amazon.geo.mapsv2
com.amazon.geo.mapsv3.resources
com.amazon.geo.mapsv3.services
com.here.odnp.service
com.amazon.hedwig
com.amazon.csapp
com.amazon.readynowcore
com.amazon.recess
com.amazon.wallpaper
com.amazon.firespotlight
com.amazon.kindle.kso
com.amazon.kindle.unifiedSearch
com.amazon.kor.demo
com.amazon.legalsettings
com.amazon.logan
com.amazon.speakscreen
amazon.jackson19
com.amazon.aca
com.amazon.advertisingidsettings
com.amazon.hybridadidservice
com.amazon.alta.h2clientservice
com.amazon.h2settingsfortablet
com.amazon.application.compatibility.enforcer
com.amazon.appverification
com.amazon.charles
com.amazon.client.metrics
com.amazon.client.metrics.api
com.amazon.device.metrics
com.amazon.minerva.client.api
com.amazon.platform
com.amazon.platform.fdra
com.amazon.wirelessmetrics.service
com.amazon.cloud9.contentservice
com.amazon.cloud9.systembrowserprovider
com.amazon.communication.discovery
com.amazon.connectivitydiag
com.amazon.dcp.contracts.framework.library
com.amazon.dcp.contracts.library
com.amazon.dcpms.fos.service
com.amazon.device.messaging
com.amazon.device.sale.service
com.amazon.device.sync
com.amazon.device.sync.sdk.internal
com.amazon.sync.provider.ipc
com.amazon.sync.service
com.amazon.diode
com.amazon.dp.contacts
com.amazon.dp.fbcontacts
com.amazon.dp.logger
com.amazon.dpcclient
com.amazon.fireos.cirruscloud
com.amazon.identity.auth.device.authorization
com.amazon.kindle.rdmdeviceadmin
com.amazon.media.session.monitor
com.amazon.nimh
com.amazon.ods.kindleconnect
com.amazon.pm
com.amazon.providers.contentsupport
com.amazon.securitysyncclient
com.amazon.sharingservice.android.client.proxy
com.amazon.shpm
com.amazon.tcomm
com.amazon.tcomm.client
com.amazon.tcomm.jackson
com.amazon.whisperlink.core.android
com.amazon.whisperplay.contracts
com.amazon.whisperplay.service.install
com.amazon.watson
com.fireos.arcus.proxy
com.fireos.usagestats.proxy
com.android.bookmarkprovider
com.android.carrierconfig
com.android.dreams.basic
com.android.email
com.android.protips
com.android.providers.downloads.ui
com.android.providers.partnerbookmarks
com.android.quicksearchbox
com.amazon.device.software.ota
com.amazon.device.software.ota.override
com.amazon.kindle.otter.oobe
com.amazon.kindle.otter.oobe.forced.ota
"

# Je installiertem Paket zwei Griffe: `pm disable-user` (schaltet ab, geht
# nicht bei „protected" Paketen) und `pm suspend` (hält an: keine Oberfläche,
# keine Benachrichtigungen — geht auch bei protected). Zurück mit
# `pm unsuspend <paket>` + `pm enable --user 0 <paket>`.
entschlacken() {
  local installiert dis=0 sus=0 nichts=0 fehlt=0 p r1 r2 ok1 ok2
  installiert=$(adb shell pm list packages --user 0 2>/dev/null | tr -d '\r' | sed 's/^package://')
  for p in $AMAZON_APPS; do
    # Here-String statt Pipe: `grep -q` bricht beim Treffer ab und würde
    # `printf` unter pipefail in SIGPIPE schicken (siehe Konten-Prüfung oben).
    if ! grep -qxF -- "$p" <<<"$installiert"; then fehlt=$((fehlt+1)); continue; fi
    r1=$(adb shell pm disable-user --user 0 "$p" 2>&1 | tr -d '\r' || true)
    r2=$(adb shell pm suspend "$p" 2>&1 | tr -d '\r' || true)
    ok1=0; ok2=0
    case "$r1" in *"new state: disabled"*) ok1=1; dis=$((dis+1)) ;; esac
    case "$r2" in *"suspended state: true"*) ok2=1; sus=$((sus+1)) ;; esac
    if [ "$ok1" = 1 ] && [ "$ok2" = 1 ]; then echo "  aus+angehalten $p"
    elif [ "$ok2" = 1 ]; then echo "  angehalten     $p (deaktivieren verweigert: protected)"
    elif [ "$ok1" = 1 ]; then echo "  aus            $p (anhalten verweigert)"
    else echo "  verweigert     $p"; nichts=$((nichts+1)); fi
  done
  echo "Entschlackt: $dis deaktiviert, $sus angehalten, $nichts verweigert, $fehlt nicht vorhanden."
}

echo "Geräte:"; adb devices
n=$(adb devices | grep -c 'device$' || true)
[ "$n" -eq 1 ] || { echo "Genau ein Tablet per USB anschließen (gefunden: $n)." >&2; exit 1; }

if [ -n "$WLAN_SSID" ]; then echo "WLAN einrichten: $WLAN_SSID"; wlan_einrichten; fi

# Device Owner geht nur ohne eingerichtete Konten. `grep -q` bricht bei einem
# Treffer die Pipe früh ab und schickt `adb` unter `set -o pipefail` in
# SIGPIPE — mit einer Variable zwischenspeichern statt direkt zu pipen.
# Nur Warnung: Fire OS 8 hat auch ohne Anmeldung drei interne Konten (Typ
# `amazon.account`), und der Besitzer-Schritt unten ist nicht mehr fatal.
konten=$(adb shell dumpsys account 2>/dev/null || true)
case "$konten" in
  *'Account {'*)
    echo "Konten auf dem Tablet gefunden – der Gerätebesitzer-Schritt wird damit scheitern (auf Fire OS 8 normal). Bei einem angemeldeten Amazon-Konto: abmelden." >&2
    ;;
esac

# Eine noch fixierte alte Fassung blockiert den Start der neuen („Lock Task
# Mode violation", Fehlercode 101) — vorher lösen, harmlos wenn nicht fixiert.
adb shell am task lock stop >/dev/null 2>&1 || true
echo "APK installieren: $APK"
if ! adb install -r "$APK"; then
  echo "adb install fehlgeschlagen." >&2
  exit 1
fi

echo "Gerätebesitzer setzen"
# Auf Fire OS 8 scheitert das IMMER (Kindersicherung ist Profile Owner, auch
# nach Werksreset) — dann läuft die Einrichtung ohne Besitzer weiter.
if ! adb shell dpm set-device-owner "$PAKET/.kiosk.KioskAdminReceiver"; then
  echo "Gerätebesitzer konnte nicht gesetzt werden. Auf Fire OS 8 normal (Kindersicherung ist Profile Owner); sonst: noch ein Konto, oder schon ein anderer Besitzer. Weiter ohne Besitzer." >&2
fi

# Ohne Besitzer erledigt die App das nicht selbst: Bildschirm am Ladekabel
# an, Sperrbildschirm (mit Werbung) aus, und der Fire-OS-Schalter
# „Touch-Funktion deaktivieren" der Kindersicherung aus — sonst schluckt der
# Toddler Mode beim Anheften jeden Touch (Feldtest 08.09.2026).
echo "Bildschirm: Wachhalten am Ladekabel, Sperrbildschirm aus, Touch beim Fixieren erlauben"
adb shell settings put global stay_on_while_plugged_in 7 || true
adb shell locksettings set-disabled true || true
adb shell settings put secure toddler_mode_default_value 0 || true
# Autostart ohne Gerätebesitzer: Android 11 bricht Activity-Starts aus
# BOOT_COMPLETED ab („Abort background activity starts"); dieser
# Konfigurationswert hebt das gerätweit auf (Feldtest 08.09.2026: überlebt
# Neustarts, SYSTEM_ALERT_WINDOW und der alte Entwickler-Schalter halfen
# auf Fire OS nicht). Auf einem Zähl-Tablet unbedenklich.
echo "Autostart: Hintergrund-Aktivitätsstarts erlauben"
adb shell device_config put activity_manager default_background_activity_starts_enabled true || true
# Ohne Besitzer zeigt Android bei jedem Fixieren den Dialog „App ist auf dem
# Bildschirm fixiert" — der Bedienungshilfe-Dienst der App tippt „Verstanden"
# selbst. Bestehende Dienste bleiben erhalten.
echo "Fixieren: Bestätigungsdienst einschalten"
dienst="$PAKET/.kiosk.BestaetigungsDienst"
alt=$(adb shell settings get secure enabled_accessibility_services 2>/dev/null | tr -d '\r' || true)
case "$alt" in
  ""|null) neu="$dienst" ;;
  *"$dienst"*) neu="$alt" ;;
  *) neu="$alt:$dienst" ;;
esac
adb shell settings put secure enabled_accessibility_services "$neu" || true
adb shell settings put secure accessibility_enabled 1 || true
# Akku: Energiesparmodus. Fire OS setzt `low_power` beim Boot am Ladekabel
# zurück — verlässlich ist die Automatik-Schwelle: bei 100 % greift der
# Sparmodus, sobald das Kabel ab ist (simuliert geprüft 08.09.2026). Die
# Helligkeit setzt die App selbst für ihr Fenster (30 %).
echo "Akku: Energiesparmodus (Automatik ab 100 %, sticky)"
adb shell settings put global low_power_trigger_level 100 || true
adb shell settings put global automatic_power_save_mode 0 || true
adb shell settings put global low_power_sticky 1 || true
adb shell settings put global low_power 1 || true
# Der Sparmodus schaltet sonst den Nachtmodus mit ein (Review-Befund).
adb shell settings put global battery_saver_constants "enable_night_mode=false" || true
if [ "$ENTSCHLACKEN" = "behalten" ]; then echo "Amazon-Apps bleiben (behalten)."
else echo "Amazon-Apps entfernen (Alexa, Video, Kindle, OTA …)"; entschlacken; fi
echo "WebView-Stand:"
for p in com.amazon.webview.chromium com.google.android.webview com.android.webview; do
  adb shell dumpsys package "$p" 2>/dev/null | grep versionName | sed "s/^/  $p /" || true
done
echo "App starten"; adb shell am start -n "$PAKET/.KioskActivity"
echo "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
