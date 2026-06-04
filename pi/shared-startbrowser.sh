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

echo "$(date) - Startbrowser (shared, auto-reconnect v4: Subnetz-Scan + Cloud-Log) gestartet" >> "$LOG"

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
# Gemerkte IP in einer DATEI (nicht als Shell-Variable!): discover()/btslight_ip()
# laufen über $(…) in einer Subshell – eine Variable würde dort verloren gehen,
# sodass JEDER Durchlauf neu (langsam, flaky) aufgelöst hätte. Die Datei übersteht
# die Subshell → genau EINE Auflösung, danach immer die schnelle IP.
BTSLIGHT_CACHE="/tmp/btslight_ip"

# Cloud-Log: das Verbindungslog periodisch an badhub.de schicken, damit ein
# Monitor-Pi im Turnierbetrieb AUS DER FERNE diagnostizierbar ist (ohne die
# SD-Karte zu ziehen). Geräte-ID = Pi-Seriennummer. Selber verbandsweiter
# Bearer-Token wie bts-light. Scheitert STILL, wenn kein Internet da ist
# (z. B. Heim-Test ohne Uplink) – dann steht das Log eben nur lokal.
SERIAL=$(awk -F': ' '/Serial/{print $2}' /proc/cpuinfo 2>/dev/null | tr -d '[:space:]')
DEVICE_ID="pi-${SERIAL}"
PILOG_URL="https://badhub.de/api/pi_log.php"
PILOG_TOKEN="d896d5c45f1dfe72d324be2da0dcc8031e447809f9a3c1ce"

# Letzte ~800 Logzeilen in die Cloud schieben; kurzer Timeout, Fehler ignorieren.
upload_log() {
  command -v curl >/dev/null 2>&1 || return 0
  [ -n "$SERIAL" ] || return 0
  tail -n 800 "$LOG" 2>/dev/null | curl -s --max-time 8 -X POST \
    -H "Authorization: Bearer ${PILOG_TOKEN}" \
    -H "X-Device-Id: ${DEVICE_ID}" \
    -H "Content-Type: text/plain" \
    --data-binary @- "$PILOG_URL" >/dev/null 2>&1 || true
}

# Erreichbar? curl (echte HTTP-Antwort, großzügiges Timeout), sonst /dev/tcp.
# KEIN ping-Fallback: Windows beantwortet ICMP standardmäßig nicht.
reachable() {
  local probe="$1" code host port
  if command -v curl >/dev/null 2>&1; then
    # Kurz: curl prüft nur IPs (bts-light wird vorab per getent aufgelöst),
    # daher genügen knappe Timeouts → schnelle Discovery-Runden.
    code=$(curl -k -s -o /dev/null -w '%{http_code}' --connect-timeout 1 --max-time 2 "$probe" 2>/dev/null)
    [ -n "$code" ] && [ "$code" != "000" ]
  else
    host=$(echo "$probe" | sed -E 's#^[a-z]+://([^/:]+).*#\1#')
    port=$(echo "$probe" | sed -nE 's#^[a-z]+://[^/:]+:([0-9]+).*#\1#p'); [ -z "$port" ] && port=80
    timeout 3 bash -c "echo > /dev/tcp/$host/$port" 2>/dev/null
  fi
}

# Subnetz nach bts-light absuchen: jede IP im eigenen /24 auf :8088/health.
# UNABHÄNGIG von mDNS – das ist über WLAN das schwächste Glied (getent hing im
# Feld minutenlang). Der Scan findet den Server am offenen Port direkt.
# Parallel in Blöcken zu 30, Abbruch beim ersten Treffer.
scan_subnet() {
  local prefix n hit="/tmp/btslight_scan_hit"
  prefix=$(hostname -I 2>/dev/null | tr ' ' '\n' \
           | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$' | head -1 | sed -E 's/\.[0-9]+$/./')
  [ -z "$prefix" ] && return 1
  rm -f "$hit"
  for n in $(seq 1 254); do
    ( reachable "http://${prefix}${n}:$BTSLIGHT_PORT/health" && echo "${prefix}${n}" > "$hit" ) &
    if [ $((n % 30)) -eq 0 ]; then wait; [ -s "$hit" ] && break; fi
  done
  wait
  [ -s "$hit" ] && { head -1 "$hit"; return 0; }
  return 1
}

# bts-light-IP ermitteln. Reihenfolge nach Zuverlässigkeit:
#   1) gemerkte IP (sofort), 2) Subnetz-Scan (zuverlässig), 3) mDNS NUR mit timeout.
btslight_ip() {
  local ip
  # 1) Gemerkte IP wiederverwenden, solange erreichbar (keine Neuauflösung!).
  #    Cache NICHT löschen bei kurzem Aussetzer – sofort wiederverwenden, sobald zurück.
  ip=$(cat "$BTSLIGHT_CACHE" 2>/dev/null)
  if [ -n "$ip" ] && reachable "http://$ip:$BTSLIGHT_PORT/health"; then
    echo "$ip"; return 0
  fi
  # 2) Subnetz-Scan nach dem offenen Port (zuverlässig, ohne mDNS).
  ip=$(scan_subnet)
  if [ -n "$ip" ]; then
    echo "$ip" > "$BTSLIGHT_CACHE"
    echo "$(date) - bts-light per Scan gefunden → $ip" >> "$LOG"
    echo "$ip"; return 0
  fi
  # 3) mDNS als Fallback – IMMER mit timeout, darf NIE hängen.
  ip=$(timeout 3 getent hosts "$BTSLIGHT_NAME" 2>/dev/null | awk '{print $1; exit}')
  if [ -z "$ip" ] && command -v avahi-resolve-host-name >/dev/null 2>&1; then
    ip=$(timeout 3 avahi-resolve-host-name -4 "$BTSLIGHT_NAME" 2>/dev/null | awk '{print $2; exit}')
  fi
  if [ -n "$ip" ] && reachable "http://$ip:$BTSLIGHT_PORT/health"; then
    echo "$ip" > "$BTSLIGHT_CACHE"
    echo "$(date) - bts-light aufgelöst (mDNS): $BTSLIGHT_NAME → $ip" >> "$LOG"
    echo "$ip"; return 0
  fi
  return 1
}

# Erste erreichbare KIOSK-URL (oder leer + Rückgabe 1).
discover() {
  local S KIOSK PROBE ip
  # 1) feste BTS/CourtSpot-Server zuerst
  for S in "${SERVERS[@]}"; do
    KIOSK="${S%%|*}"; PROBE="${S##*|}"
    if reachable "$PROBE"; then echo "$KIOSK"; return 0; fi
  done
  # 2) bts-light per aufgelöster IP (stabil), inkl. eindeutiger Geräte-Kennung
  ip="$(btslight_ip)"
  if [ -n "$ip" ]; then
    if [ -n "$SERIAL" ]; then
      echo "http://$ip:$BTSLIGHT_PORT/monitor?device=${DEVICE_ID}"
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
#    WICHTIG (Lehre aus dem Feld): einen EINZELNEN Aussetzer NICHT sofort als
#    "Server weg" werten – sonst flackert der Kiosk zum Desktop und wieder zurück.
#    Erst nach MISS_LIMIT erfolglosen Runden (≈30 s) beenden. So bleibt ein
#    laufender Kiosk bei kurzem WLAN-Wackler einfach stehen.
CUR=""
MISS=0
MISS_LIMIT=3
TICK=0
while true; do
  TICK=$((TICK + 1))
  BEST="$(discover || true)"
  if [ -n "$BEST" ]; then
    MISS=0
    if [ "$BEST" != "$CUR" ] || ! pgrep -f chromium-browser >/dev/null 2>&1; then
      echo "$(date) - Anzeige: $BEST" >> "$LOG"
      launch "$BEST"
      CUR="$BEST"
    fi
  else
    MISS=$((MISS + 1))
    # Heartbeat: alle ~60 s eine Zeile, damit das Log bei Suche nicht stumm bleibt.
    if [ $((MISS % 6)) -eq 1 ]; then
      echo "$(date) - noch kein Server – suche weiter (Versuch $MISS)" >> "$LOG"
    fi
    if [ -n "$CUR" ] && [ "$MISS" -ge "$MISS_LIMIT" ]; then
      echo "$(date) - Server seit $MISS Versuchen weg – Kiosk beendet, suche weiter" >> "$LOG"
      pkill -f chromium-browser 2>/dev/null
      CUR=""
    fi
  fi
  # Verbindungslog in die Cloud: gleich beim ersten Durchlauf (Boot-Info schnell
  # sichtbar) und danach alle ~5 min. Im Hintergrund → blockiert die Schleife nicht.
  if [ $(( (TICK - 1) % 30 )) -eq 0 ]; then upload_log & fi
  sleep 10
done
