#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────
# Gemeinsamer Kiosk-Launcher für ein Pi-Image, das SOWOHL Tilos BTS-Server
# ALS AUCH bts-light bedient (Verleih-Set, gemischte Turniere).
#
# Drop-in-Ersatz für /home/pi/startbrowser.sh auf Tilos Pi-Image
# (Raspberry Pi OS Desktop, Autologin pi → LXDE-Autostart ruft dieses Skript).
#
# Prinzip (unverändert von Tilos Original): Der Pi lädt NUR einen Chromium-
# Kiosk; der gesamte Inhalt kommt vom Server. Beim Boot wird die SERVERS-Liste
# der Reihe nach geprüft (Host erreichbar?) — der erste Treffer gewinnt und
# wird im Vollbild geladen. So entscheidet sich allein über „welcher Server
# ist im Netz", ob BTS oder bts-light angezeigt wird. Kein doppeltes OS.
#
# Gegenüber Tilos Original geändert:
#   1. bts-light (mDNS) als zusätzlicher SERVERS-Eintrag.
#   2. Netz-Warten ohne Internet-Zwang (reines bts-light-LAN hat ggf. kein
#      Internet) — wartet auf eine eigene IP statt auf `ping 8.8.8.8`.
#   3. Discovery prüft Host-Erreichbarkeit per ping (wie Tilo); mDNS-Name
#      `bts-light.local` wird über avahi aufgelöst (im Desktop-Image vorhanden).
# ─────────────────────────────────────────────────────────────────────────
export DISPLAY=:0
export XAUTHORITY=/home/pi/.Xauthority
LOG=/home/pi/startbrowser.log

echo "$(date) - Startbrowser (shared BTS/bts-light) gestartet" >> "$LOG"

# 1) Auf Netz warten — NICHT auf Internet, sondern auf eine eigene IP. Damit
#    funktioniert auch ein reines bts-light-LAN (Laptop+Router, kein Internet).
#    Timeout 120 s, danach versuchen wir die Discovery trotzdem.
echo "$(date) - Warte auf WLAN/IP..." >> "$LOG"
for _ in $(seq 1 60); do
    # Eigene (Nicht-Loopback-)IP vorhanden?
    if hostname -I 2>/dev/null | grep -qE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'; then
        break
    fi
    sleep 2
done
echo "$(date) - Netz bereit: $(hostname -I 2>/dev/null)" >> "$LOG"

# 2) Serverliste in Prüf-Reihenfolge: kiosk_url|probe_url
#    Echte BTS-/CourtSpot-Server zuerst (feste IPs), bts-light als Fallback
#    (mDNS — greift, wenn kein BTS-Server im Netz ist). Die probe_url wird
#    per HTTP geprüft: antwortet ein ECHTER Dienst (beliebiger HTTP-Code),
#    gewinnt der Eintrag. So genügt nicht, dass irgendwer auf die IP „pingt".
SERVERS=(
  "https://192.168.16.2:4433/d1|https://192.168.16.2:4433/d1"
  "https://192.168.26.2:4433/d1|https://192.168.26.2:4433/d1"
  "https://192.168.36.2:4433/d1|https://192.168.36.2:4433/d1"
  "http://192.168.16.3/regio/Update-Verzeichnis/bup/#courtspot&display|http://192.168.16.3/"
  "http://bts-light.local:8088/monitor|http://bts-light.local:8088/health"
)

# Erreichbarkeit eines Dienstes prüfen: bevorzugt curl (echte HTTP-Antwort),
# Fallback ping (falls curl fehlt). Liefert 0 = erreichbar.
reachable() {
  local probe="$1" host
  if command -v curl >/dev/null 2>&1; then
    local code
    code=$(curl -k -s -o /dev/null -w '%{http_code}' --max-time 2 "$probe" 2>/dev/null)
    [ -n "$code" ] && [ "$code" != "000" ]
  else
    host=$(echo "$probe" | sed -E 's#^[a-z]+://([^/:]+).*#\1#')
    ping -c1 -W1 "$host" &>/dev/null
  fi
}

URL=""
# Bis zu 5 Discovery-Runden (Geräte/mDNS/Server brauchen nach dem WLAN-Join kurz).
for _round in 1 2 3 4 5; do
  for S in "${SERVERS[@]}"; do
    KIOSK="${S%%|*}"; PROBE="${S##*|}"
    if reachable "$PROBE"; then
      URL="$KIOSK"
      echo "$(date) - Gefunden: $URL (Probe $PROBE)" >> "$LOG"
      break 2
    fi
  done
  sleep 2
done

if [ -z "$URL" ]; then
    echo "$(date) - Kein Server erreichbar" >> "$LOG"
    xmessage -center "🚨 Kein Badminton-Server gefunden (BTS oder bts-light)!"
    exit 1
fi

# 3) Stabile Geräte-Kennung für bts-light anhängen (Pi-Seriennummer), damit der
#    Monitor in bts-light dauerhaft dasselbe Gerät bleibt. Für BTS unverändert.
case "$URL" in
  *bts-light.local*)
    if ! echo "$URL" | grep -q "device="; then
      SERIAL=$(awk -F': ' '/Serial/{print $2}' /proc/cpuinfo 2>/dev/null | tr -d '[:space:]')
      SEP="?"; case "$URL" in *\?*) SEP="&";; esac
      [ -n "$SERIAL" ] && URL="${URL}${SEP}device=pi-${SERIAL}"
      echo "$(date) - bts-light Geraet: $URL" >> "$LOG"
    fi
    ;;
esac

# 4) Chromium-Kiosk starten (Tilos Flags + Translate/Sprach-Flags wie im
#    bts-light-Setup, damit kein „Übersetzen"-Balken erscheint).
chromium-browser \
  --noerrdialogs \
  --disable-infobars \
  --disable-session-crashed-bubble \
  --ignore-certificate-errors \
  --disable-features=Translate,TranslateUI \
  --disable-translate \
  --lang=de-DE --accept-lang=de-DE,de \
  --kiosk "$URL" &
