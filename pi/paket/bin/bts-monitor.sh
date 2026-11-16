#!/usr/bin/env bash
# Court-Monitor-Kiosk – aufgerufen vom Autostart (eingerichtet von setup-monitor.sh).
# Browser und Hinweisseite werden zur Laufzeit ermittelt (früher bei der
# Einrichtung fest eingetragen): Dieselbe Datei läuft so unverändert im Image
# und in einem Paket, das an einem anderen Ort liegt.
BROWSER="$(command -v chromium-browser || command -v chromium || true)"
HIER="$(dirname "$(readlink -f "$0")")"
NO_URL_DATEI="$(readlink -f "$HIER/../share/no-url.html" 2>/dev/null || true)"
# Einrichtung am alten Ort: Skript in ~/.local/bin, Seite in ~/.local/share/bts-monitor.
[ -f "$NO_URL_DATEI" ] || NO_URL_DATEI="$HOME/.local/share/bts-monitor/no-url.html"
NO_URL_PAGE="file://$NO_URL_DATEI"
# Nur für den Test (scripts/test-pi-quelle.mjs): Werte ausgeben und enden.
if [ "${BTS_NUR_WERTE:-}" = "1" ]; then
  printf 'BROWSER=%s\nNO_URL_PAGE=%s\n' "$BROWSER" "$NO_URL_PAGE"
  exit 0
fi
set -u
LOG="${XDG_RUNTIME_DIR:-/tmp}/bts-monitor.log"
CACHE="${XDG_RUNTIME_DIR:-/tmp}/bts-monitor-ip"
# Adresse des Turnier-PCs für den CEC-Dienst (bts-cec.sh fragt dort nach, ob
# der TV an- oder ausgeschaltet werden soll). Nur im Hallennetz.
PCDATEI="${XDG_RUNTIME_DIR:-/tmp}/bts-monitor-pc"
PORT_STANDARD=8088
log() { echo "$(date '+%F %T') - $*" >> "$LOG"; }

# Monitor-Adresse aus der Boot-Partition, bei jedem Start frisch gelesen
# (am Computer editierbar, Leerzeichen werden entfernt).
BOOT_DIR="/boot/firmware"; [ -d "$BOOT_DIR" ] || BOOT_DIR="/boot"
URL="$(tr -d '[:space:]' < "$BOOT_DIR/bts-monitor-url.txt" 2>/dev/null || true)"

# Geräte-Kennung = Hardware-Seriennummer (eine geklonte Karte bleibt je Pi
# eindeutig). Format pi-<serial> wie beim früheren gemeinsamen Image, damit
# bereits zugewiesene Monitore nach dem Neubespielen ihr Feld behalten.
SERIAL="$(awk -F': ' '/^Serial/{print $2}' /proc/cpuinfo 2>/dev/null | tr -d '[:space:]')"
[ -z "$SERIAL" ] && SERIAL="$(tr -d '\0' 2>/dev/null < /sys/firmware/devicetree/base/serial-number || true)"
[ -z "$SERIAL" ] && SERIAL="$(hostname | tr -d '[:space:]')"
DEVICE_ID="pi-${SERIAL}"

# Adresse zerlegen: http://HOST[:PORT][/PFAD][?QUERY]
REST="${URL#*://}"
ANFRAGE=""
case "$REST" in *\?*) ANFRAGE="?${REST#*\?}"; REST="${REST%%\?*}" ;; esac
HOSTPORT="${REST%%/*}"
PFAD="/${REST#*/}"; [ "$REST" = "$HOSTPORT" ] && PFAD="/monitor"
PFAD="${PFAD}${ANFRAGE}"
HOST="${HOSTPORT%%:*}"
PORT="${HOSTPORT##*:}"; [ "$PORT" = "$HOSTPORT" ] && PORT=""

# Erreichbar? Echte HTTP-Antwort; kein ping (Windows beantwortet ICMP nicht).
erreichbar() {
  local code
  code=$(curl -s -o /dev/null -w '%{http_code}' --connect-timeout 1 --max-time 2 "$1" 2>/dev/null)
  [ -n "$code" ] && [ "$code" != "000" ]
}

# Eigenes /24 parallel auf :PORT/health absuchen – unabhängig von mDNS,
# das über WLAN im Feld minutenlang hing.
subnetz_suchen() {
  local praefix n treffer="${XDG_RUNTIME_DIR:-/tmp}/bts-monitor-scan"
  praefix=$(hostname -I 2>/dev/null | tr ' ' '\n' \
    | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$' | head -1 | sed -E 's/\.[0-9]+$/./')
  [ -z "$praefix" ] && return 1
  rm -f "$treffer"
  for n in $(seq 1 254); do
    ( erreichbar "http://${praefix}${n}:$1/health" && echo "${praefix}${n}" > "$treffer" ) &
    if [ $((n % 30)) -eq 0 ]; then wait; [ -s "$treffer" ] && break; fi
  done
  wait
  [ -s "$treffer" ] && { head -1 "$treffer"; return 0; }
  return 1
}

# Turnier-PC finden: gemerkte IP → Subnetz-Suche → mDNS (immer mit timeout).
pc_ip() {
  local ip port="$1"
  ip=$(cat "$CACHE" 2>/dev/null)
  if [ -n "$ip" ] && erreichbar "http://$ip:$port/health"; then echo "$ip"; return 0; fi
  ip=$(subnetz_suchen "$port")
  if [ -z "$ip" ]; then
    ip=$(timeout 3 getent hosts "$HOST" 2>/dev/null | awk '{print $1; exit}')
    [ -n "$ip" ] && ! erreichbar "http://$ip:$port/health" && ip=""
  fi
  [ -n "$ip" ] || return 1
  [ "$ip" != "$(cat "$CACHE" 2>/dev/null)" ] && log "Turnier-PC gefunden: $ip"
  echo "$ip" > "$CACHE"
  echo "$ip"
}

# Ziel-Adresse dieses Durchlaufs (leer = nichts erreichbar).
ziel() {
  local ip port sep
  case "$PFAD" in *\?*) sep="&" ;; *) sep="?" ;; esac
  if [ "$HOST" = "bts-light.local" ]; then
    # Standardfall: den PC selbst suchen, Pfad und Parameter (z. B. ?ansage=)
    # aus der Datei übernehmen.
    port="${PORT:-$PORT_STANDARD}"
    ip=$(pc_ip "$port") || return 1
    echo "http://$ip:$port" > "$PCDATEI"
    echo "http://$ip:$port${PFAD}${sep}device=${DEVICE_ID}"
  else
    # Feste Adresse (IP oder Cloud): so nehmen, wie sie in der Datei steht.
    erreichbar "$URL" || return 1
    # Feste Adresse im Hallennetz → der CEC-Dienst darf dort nachfragen;
    # eine Cloud-Adresse (https) kennt die Abfrage nicht.
    case "$URL" in
      http://*) echo "http://$HOSTPORT" > "$PCDATEI" ;;
      *) rm -f "$PCDATEI" ;;
    esac
    echo "${URL}${sep}device=${DEVICE_ID}"
  fi
}

# Log an den Turnier-PC (der leitet es an die Cloud weiter) – nur im LAN-Fall.
log_senden() {
  local ip port="${PORT:-$PORT_STANDARD}"
  ip=$(cat "$CACHE" 2>/dev/null); [ -n "$ip" ] || return 0
  tail -n 800 "$LOG" 2>/dev/null | curl -s --max-time 8 -X POST \
    -H "Content-Type: text/plain" --data-binary @- \
    "http://${ip}:${port}/pi-log?device=${DEVICE_ID}" >/dev/null 2>&1 || true
}

BROWSER_PID=""
starten() {
  [ -n "$BROWSER_PID" ] && kill "$BROWSER_PID" 2>/dev/null && sleep 1
  # Nach einem Stromausfall Chromiums „Wiederherstellen?"-Frage unterdrücken.
  local pref="$HOME/.config/chromium/Default/Preferences"
  [ -f "$pref" ] && sed -i \
    's/"exited_cleanly":false/"exited_cleanly":true/;s/"exit_type":"[^"]*"/"exit_type":"Normal"/' \
    "$pref" 2>/dev/null
  # --autoplay-policy: Ton ohne Tippen — die Monitor-Seiten sprechen mit
  # ?ansage=<Halle> in der Adresse die Ansagen ihrer Halle (Spec
  # ansage-im-browser, E1b). Am TV gibt es niemanden, der „Ton freischalten"
  # drückt; ohne das Flag bleibt die Seite stumm und zeigt einen Hinweis.
  "$BROWSER" --no-memcheck \
    --kiosk --incognito --noerrdialogs --disable-infobars \
    --disable-session-crashed-bubble --no-first-run \
    --check-for-update-interval=31536000 \
    --disable-features=Translate,TranslateUI --disable-translate \
    --lang=de-DE --accept-lang=de-DE,de \
    --autoplay-policy=no-user-gesture-required \
    "$1" >/dev/null 2>&1 &
  BROWSER_PID=$!
}

# Bild im 16:9-Modus. Manche HD-Ready-Fernseher (1366×768) melden dem Pi
# 1024×768 als Wunschmodus; der TV zieht das 4:3-Bild dann breit — Kreise
# werden oval, und die Namen haben ein Viertel weniger Platz (Feldfoto
# 12.11.2026). Bietet der TV einen 16:9-Modus an, nehmen wir den größten.
# Rührt nichts an, wenn das Bild schon breit ist, der TV keinen solchen
# Modus meldet oder kein X läuft (Desktop-Variante mit Wayland).
# Liest die Ausgabe von `xrandr --current` und gibt „<Ausgang> <Modus>" aus.
breitbild_modus() {
  awk '
    /^[^ ]+ connected/ {
      aus = ""; if (gefunden) next
      for (i = 3; i <= NF; i++) if ($i ~ /^[0-9]+x[0-9]+\+/) {
        split($i, t, /[x+]/)
        if (t[1] * 9 < t[2] * 16 * 0.95) { aus = $1; gefunden = 1 }
      }
      next
    }
    /^[^ ]/ { aus = "" ; next }
    aus != "" && $1 ~ /^[0-9]+x[0-9]+$/ {
      split($1, m, "x"); v = m[1] / m[2]
      if (v > 1.7 && v < 1.8 && m[1] * m[2] > beste) { beste = m[1] * m[2]; modus = $1; ziel = aus }
    }
    END { if (modus != "") print ziel, modus }
  '
}
if [ -z "${WAYLAND_DISPLAY:-}" ] && command -v xrandr >/dev/null; then
  WAHL="$(xrandr --current 2>/dev/null | breitbild_modus)"
  if [ -n "$WAHL" ]; then
    # shellcheck disable=SC2086
    set -- $WAHL
    xrandr --output "$1" --mode "$2" 2>/dev/null \
      && log "Bildmodus auf $2 gestellt (TV meldete 4:3)" \
      || log "Bildmodus $2 ließ sich nicht setzen"
    set --
  fi
fi

command -v unclutter >/dev/null && unclutter -idle 0.5 -root &
log "Kiosk gestartet (Gerät $DEVICE_ID, Adresse ${URL:-keine})"

if [ -z "$URL" ]; then
  # Keine Adresse hinterlegt → Hinweisseite, Chromium bei Absturz neu starten.
  while true; do
    { [ -z "$BROWSER_PID" ] || ! kill -0 "$BROWSER_PID" 2>/dev/null; } && starten "$NO_URL_PAGE"
    sleep 10
  done
fi

# Auf eine eigene IP warten (kein Internet-Zwang).
for _ in $(seq 1 60); do
  hostname -I 2>/dev/null | grep -qE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' && break
  sleep 2
done
log "Netz: $(hostname -I 2>/dev/null)"

# Dauerschleife. Ein EINZELNER Aussetzer beendet den Kiosk nicht (sonst
# flackert er bei WLAN-Wacklern) – erst nach FEHL_GRENZE Runden. Eine
# erfolglose Runde dauert mit Subnetz-Suche ~20–30 s, zusammen ~1–1,5 min.
AKTUELL=""
FEHL=0
FEHL_GRENZE=3
TAKT=0
while true; do
  TAKT=$((TAKT + 1))
  BESTE="$(ziel || true)"
  if [ -n "$BESTE" ]; then
    FEHL=0
    if [ "$BESTE" != "$AKTUELL" ] || [ -z "$BROWSER_PID" ] || ! kill -0 "$BROWSER_PID" 2>/dev/null; then
      log "Anzeige: $BESTE"
      starten "$BESTE"
      AKTUELL="$BESTE"
    fi
  else
    FEHL=$((FEHL + 1))
    [ $((FEHL % 6)) -eq 1 ] && log "noch kein Turnier-PC – suche weiter (Versuch $FEHL)"
    if [ -n "$AKTUELL" ] && [ "$FEHL" -ge "$FEHL_GRENZE" ]; then
      log "Turnier-PC seit $FEHL Versuchen weg – Kiosk beendet, suche weiter"
      kill "$BROWSER_PID" 2>/dev/null
      AKTUELL=""
    fi
  fi
  # Log gleich beim ersten Durchlauf und danach alle ~5 min an den PC.
  if [ $(( (TAKT - 1) % 30 )) -eq 0 ]; then
    log_senden &
    # Log kurz halten, damit es die Karte nicht füllt.
    tail -n 2000 "$LOG" > "$LOG.neu" 2>/dev/null && mv "$LOG.neu" "$LOG"
  fi
  sleep 10
done
