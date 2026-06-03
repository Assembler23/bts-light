#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────
# Gemeinsamer Kiosk-Launcher: bedient SOWOHL Tilos BTS-Server ALS AUCH
# bts-light (Verleih-Set, gemischte Turniere). Drop-in für /home/pi/startbrowser.sh
# auf Tilos Pi-Image (Autologin pi → LXDE-Autostart ruft dieses Skript).
#
# Dauerschleife: sucht laufend den ersten erreichbaren Server und zeigt ihn:
#   kein Server → wartet (Desktop) und sucht weiter; gefunden → Kiosk;
#   Server-Wechsel BTS↔bts-light → umschalten; Chromium-Absturz → Neustart.
#
# Wichtig (Lehre aus dem Feld): die mDNS-Auflösung von `bts-light.local` ist über
# WLAN oft LANGSAM/flaky. Darum lösen wir den Namen EINMAL geduldig zur IP auf,
# MERKEN sie und prüfen/laden danach über die zuverlässige IP (Re-Resolve nur,
# wenn die gemerkte IP nicht mehr antwortet). curl --max-time darf nicht zu kurz
# sein, sonst schlägt die Auflösung im curl selbst fehl.
# ─────────────────────────────────────────────────────────────────────────
export DISPLAY=:0
export XAUTHORITY=/home/pi/.Xauthority
LOG=/home/pi/startbrowser.log

echo "$(date) - Startbrowser (shared, auto-reconnect v2) gestartet" >> "$LOG"

# 1) Auf eigene IP warten (kein Internet-Zwang).
for _ in $(seq 1 60); do
    if hostname -I 2>/dev/null | grep -qE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'; then break; fi
    sleep 2
done
echo "$(date) - Netz: $(hostname -I 2>/dev/null)" >> "$LOG"

# Feste Server (BTS/CourtSpot): kiosk_url|probe_url – per IP, schnell.
SERVERS=(
  "https://192.168.16.2:4433/d1|https://192.168.16.2:4433/d1"
  "https://192.168.26.2:4433/d1|https://192.168.26.2:4433/d1"
  "https://192.168.36.2:4433/d1|https://192.168.36.2:4433/d1"
  "http://192.168.16.3/regio/Update-Verzeichnis/bup/#courtspot&display|http://192.168.16.3/"
)
BTSLIGHT_NAME="bts-light.local"
BTSLIGHT_PORT="8088"
BTSLIGHT_IP=""   # gemerkte, aufgelöste IP von bts-light

# Erreichbar? curl (echte HTTP-Antwort, großzügiges Timeout), sonst /dev/tcp.
# KEIN ping-Fallback: Windows beantwortet ICMP standardmäßig nicht.
reachable() {
  local probe="$1" code host port
  if command -v curl >/dev/null 2>&1; then
    code=$(curl -k -s -o /dev/null -w '%{http_code}' --max-time 5 "$probe" 2>/dev/null)
    [ -n "$code" ] && [ "$code" != "000" ]
  else
    host=$(echo "$probe" | sed -E 's#^[a-z]+://([^/:]+).*#\1#')
    port=$(echo "$probe" | sed -nE 's#^[a-z]+://[^/:]+:([0-9]+).*#\1#p'); [ -z "$port" ] && port=80
    timeout 3 bash -c "echo > /dev/tcp/$host/$port" 2>/dev/null
  fi
}

# bts-light-IP ermitteln: gemerkte IP wiederverwenden, solange erreichbar;
# sonst Namen geduldig auflösen (getent/avahi, mehrere Versuche) und merken.
btslight_ip() {
  if [ -n "$BTSLIGHT_IP" ] && reachable "http://$BTSLIGHT_IP:$BTSLIGHT_PORT/health"; then
    echo "$BTSLIGHT_IP"; return 0
  fi
  BTSLIGHT_IP=""
  local ip="" try
  for try in 1 2 3; do
    ip=$(getent hosts "$BTSLIGHT_NAME" 2>/dev/null | awk '{print $1; exit}')
    if [ -z "$ip" ] && command -v avahi-resolve-host-name >/dev/null 2>&1; then
      ip=$(avahi-resolve-host-name -4 "$BTSLIGHT_NAME" 2>/dev/null | awk '{print $2; exit}')
    fi
    if [ -n "$ip" ] && reachable "http://$ip:$BTSLIGHT_PORT/health"; then
      BTSLIGHT_IP="$ip"
      echo "$(date) - bts-light aufgelöst: $BTSLIGHT_NAME → $ip" >> "$LOG"
      echo "$ip"; return 0
    fi
    sleep 2
  done
  return 1
}

# Erste erreichbare KIOSK-URL (oder leer + Rückgabe 1).
discover() {
  local S KIOSK PROBE ip serial sep
  # 1) feste BTS/CourtSpot-Server zuerst
  for S in "${SERVERS[@]}"; do
    KIOSK="${S%%|*}"; PROBE="${S##*|}"
    if reachable "$PROBE"; then echo "$KIOSK"; return 0; fi
  done
  # 2) bts-light per aufgelöster IP (stabil), inkl. eindeutiger Geräte-Kennung
  ip="$(btslight_ip)"
  if [ -n "$ip" ]; then
    serial=$(awk -F': ' '/Serial/{print $2}' /proc/cpuinfo 2>/dev/null | tr -d '[:space:]')
    sep="?"
    if [ -n "$serial" ]; then
      echo "http://$ip:$BTSLIGHT_PORT/monitor${sep}device=pi-${serial}"
    else
      echo "http://$ip:$BTSLIGHT_PORT/monitor"
    fi
    return 0
  fi
  return 1
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
CUR=""
while true; do
  BEST="$(discover || true)"
  if [ -n "$BEST" ]; then
    if [ "$BEST" != "$CUR" ] || ! pgrep -f chromium-browser >/dev/null 2>&1; then
      echo "$(date) - Anzeige: $BEST" >> "$LOG"
      launch "$BEST"
      CUR="$BEST"
    fi
  else
    if [ -n "$CUR" ]; then
      echo "$(date) - Kein Server erreichbar – Kiosk beendet, suche weiter" >> "$LOG"
      pkill -f chromium-browser 2>/dev/null
      CUR=""
    fi
  fi
  sleep 10
done
