#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────
# Gemeinsamer Kiosk-Launcher für ein Pi-Image, das SOWOHL Tilos BTS-Server
# ALS AUCH bts-light bedient (Verleih-Set, gemischte Turniere).
#
# Drop-in-Ersatz für /home/pi/startbrowser.sh auf Tilos Pi-Image
# (Raspberry Pi OS Desktop, Autologin pi → LXDE-Autostart ruft dieses Skript).
#
# Prinzip: Der Pi lädt NUR einen Chromium-Kiosk; der Inhalt kommt vom Server.
# Eine Dauerschleife sucht laufend den ersten erreichbaren Server aus SERVERS
# und zeigt ihn an:
#   • kein Server → wartet (Desktop sichtbar), sucht weiter (gibt NICHT auf);
#   • Server gefunden → Chromium-Kiosk startet automatisch;
#   • Server wechselt (z. B. BTS geht an, während bts-light lief) → es wird
#     auf den höherprioren Server umgeschaltet (Chromium neu gestartet);
#   • Chromium abgestürzt → wird automatisch neu gestartet.
# So ist die Reihenfolge „erst Pi, dann Server" egal und ein System-Wechsel
# braucht keinen Neustart.
# ─────────────────────────────────────────────────────────────────────────
export DISPLAY=:0
export XAUTHORITY=/home/pi/.Xauthority
LOG=/home/pi/startbrowser.log

echo "$(date) - Startbrowser (shared, auto-reconnect) gestartet" >> "$LOG"

# 1) Auf eigene IP warten — NICHT auf Internet (reines bts-light-LAN hat ggf.
#    kein Internet). Timeout ~120 s, danach trotzdem in die Suchschleife.
for _ in $(seq 1 60); do
    if hostname -I 2>/dev/null | grep -qE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'; then break; fi
    sleep 2
done
echo "$(date) - Netz: $(hostname -I 2>/dev/null)" >> "$LOG"

# 2) Serverliste in Prüf-Reihenfolge: kiosk_url|probe_url
#    BTS/CourtSpot zuerst (feste IPs), bts-light als Fallback (mDNS).
SERVERS=(
  "https://192.168.16.2:4433/d1|https://192.168.16.2:4433/d1"
  "https://192.168.26.2:4433/d1|https://192.168.26.2:4433/d1"
  "https://192.168.36.2:4433/d1|https://192.168.36.2:4433/d1"
  "http://192.168.16.3/regio/Update-Verzeichnis/bup/#courtspot&display|http://192.168.16.3/"
  "http://bts-light.local:8088/monitor|http://bts-light.local:8088/health"
)

# Erreichbarkeit eines Dienstes: bevorzugt curl (echte HTTP-Antwort), sonst
# ping als Fallback. 0 = erreichbar.
reachable() {
  local probe="$1" host code
  if command -v curl >/dev/null 2>&1; then
    code=$(curl -k -s -o /dev/null -w '%{http_code}' --max-time 2 "$probe" 2>/dev/null)
    [ -n "$code" ] && [ "$code" != "000" ]
  else
    host=$(echo "$probe" | sed -E 's#^[a-z]+://([^/:]+).*#\1#')
    ping -c1 -W1 "$host" &>/dev/null
  fi
}

# Erste erreichbare KIOSK-URL ausgeben (oder leer + Rückgabe 1).
discover() {
  local S KIOSK PROBE
  for S in "${SERVERS[@]}"; do
    KIOSK="${S%%|*}"; PROBE="${S##*|}"
    if reachable "$PROBE"; then echo "$KIOSK"; return 0; fi
  done
  return 1
}

# bts-light: stabile Geräte-Kennung (Pi-Seriennummer) anhängen. BTS unverändert.
append_device() {
  local url="$1" serial sep
  case "$url" in
    *bts-light.local*)
      if ! echo "$url" | grep -q "device="; then
        serial=$(awk -F': ' '/Serial/{print $2}' /proc/cpuinfo 2>/dev/null | tr -d '[:space:]')
        sep="?"; case "$url" in *\?*) sep="&";; esac
        [ -n "$serial" ] && url="${url}${sep}device=pi-${serial}"
      fi ;;
  esac
  echo "$url"
}

# Chromium-Kiosk auf eine URL (neu) starten.
launch() {
  pkill -f chromium-browser 2>/dev/null
  sleep 1
  chromium-browser \
    --noerrdialogs \
    --disable-infobars \
    --disable-session-crashed-bubble \
    --ignore-certificate-errors \
    --disable-features=Translate,TranslateUI \
    --disable-translate \
    --lang=de-DE --accept-lang=de-DE,de \
    --kiosk "$1" &
}

# 3) Dauerschleife: immer den besten erreichbaren Server anzeigen.
CUR=""   # aktuell geladene KIOSK-URL
while true; do
  BEST="$(discover || true)"
  if [ -n "$BEST" ]; then
    BEST="$(append_device "$BEST")"
    # Neu starten, wenn sich der Server geändert hat ODER Chromium fehlt
    # (Wechsel BTS↔bts-light bzw. Absturz-Selbstheilung).
    if [ "$BEST" != "$CUR" ] || ! pgrep -f chromium-browser >/dev/null 2>&1; then
      echo "$(date) - Anzeige: $BEST" >> "$LOG"
      launch "$BEST"
      CUR="$BEST"
    fi
  else
    # Kein Server erreichbar → laufenden Kiosk beenden (Desktop sichtbar),
    # weiter suchen.
    if [ -n "$CUR" ]; then
      echo "$(date) - Kein Server erreichbar – Kiosk beendet, suche weiter" >> "$LOG"
      pkill -f chromium-browser 2>/dev/null
      CUR=""
    fi
  fi
  sleep 15
done
