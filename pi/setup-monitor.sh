#!/usr/bin/env bash
#
# bts-light Court-Monitor – Kiosk-Einrichtung für den Raspberry Pi.
#
# Verwandelt ein frisches Raspberry Pi OS in eine Vollbild-Court-Monitor-
# Anzeige. Funktioniert sowohl auf **Pi OS Desktop** (LXDE-Autostart) als
# auch auf **Pi OS Lite** (eigener X-Stack + tty1-Autologin + .xinitrc).
# Einmal ausführen, dann neu starten.
#
# Die Monitor-Adresse wird NICHT fest eingebaut, sondern bei jedem Start
# aus der Datei  bts-monitor-url.txt  auf der Boot-Partition gelesen.
# Dadurch eignet sich eine eingerichtete Karte als Master-Image: einmal
# klonen – jeder Betreiber trägt nur seine Adresse in diese eine Datei
# ein (von jedem Computer aus mit einem Texteditor editierbar).
#
# Aufruf:
#   bash setup-monitor.sh                          (interaktiv, fragt URL)
#   BTS_MONITOR_URL=http://bts-light.local:8088/monitor bash setup-monitor.sh
#   curl -fsSL https://raw.githubusercontent.com/Assembler23/bts-light/main/pi/setup-monitor.sh | bash
#
# Anleitung: docs/pi-setup.md · Master-Image: docs/pi-master-image.md

set -euo pipefail

echo "── bts-light Court-Monitor · Pi-Einrichtung ──────────────────────"

# ─── Variant-Erkennung: Pi OS Desktop oder Lite? ─────────────────────────
# Desktop hat LXDE-Session-Manager (`lxsession`). Lite hat ihn nicht.
if command -v lxsession >/dev/null 2>&1; then
  VARIANT="desktop"
else
  VARIANT="lite"
fi
echo "→ Pi OS Variante: $VARIANT"

# Karten mit dem Image vom November 2026 können noch im Nur-Lese-Modus
# laufen (inzwischen ausgebaut): Dort wären alle Änderungen nach dem
# Neustart weg.
if [ "$(findmnt -n -o FSTYPE / 2>/dev/null)" = "overlay" ]; then
  echo "FEHLER: Die Karte läuft noch im Nur-Lese-Modus – Änderungen gingen verloren."
  echo "  Datei bts-schreiben-erlaubt.txt auf der Boot-Partition anlegen"
  echo "  (sudo touch /boot/firmware/bts-schreiben-erlaubt.txt), neu starten,"
  echo "  ca. 2 Minuten warten (der Pi startet von selbst ein zweites Mal),"
  echo "  dann dieses Skript erneut ausführen."
  exit 1
fi

# Boot-Partition finden (Bookworm: /boot/firmware, älter: /boot).
BOOT_DIR="/boot/firmware"
[ -d "$BOOT_DIR" ] || BOOT_DIR="/boot"
URL_FILE="$BOOT_DIR/bts-monitor-url.txt"

# ─── 1) Monitor-Adresse ──────────────────────────────────────────────────
# Enter = Standardadresse über den festen Namen `bts-light.local`. Die
# funktioniert dank mDNS in JEDEM Turnier-WLAN, ohne feste IP – ideal
# fürs Master-Image. Eine eigene Adresse lässt sich später jederzeit in
# bts-monitor-url.txt eintragen.
DEFAULT_URL="http://bts-light.local:8088/monitor"
URL=""
if [ -t 0 ]; then
  # Interaktiv (Tastatur dran): nachfragen.
  echo
  echo 'Monitor-Adresse (steht in bts-light unter „Court-Monitore").'
  echo "Enter = Standard  $DEFAULT_URL"
  echo "(passt für jedes Turnier – ideal fürs Master-Image)."
  read -rp "Adresse: " URL || true
fi
# Fallback-Kette: Eingabe → Env-Var → Default.
[ -z "${URL:-}" ] && URL="${BTS_MONITOR_URL:-$DEFAULT_URL}"
echo "$URL" | sudo tee "$URL_FILE" >/dev/null
echo "→ gespeichert in $URL_FILE: $URL"

# ─── 2) Pakete installieren ──────────────────────────────────────────────
echo
echo "→ Pakete installieren (auf Lite dauert das ein paar Minuten)…"
sudo apt-get update -qq
# v4l-utils bringt cec-ctl mit (TV per HDMI-CEC einschalten, Abschnitt 3c).
COMMON_PKGS="unclutter fonts-noto-color-emoji v4l-utils curl"
LITE_PKGS="xserver-xorg-core xserver-xorg-input-libinput xinit x11-xserver-utils matchbox-window-manager chromium avahi-utils xdotool"

if [ "$VARIANT" = "lite" ]; then
  # shellcheck disable=SC2086  # Paketliste absichtlich ohne Quoting.
  sudo apt-get install -y --no-install-recommends $COMMON_PKGS $LITE_PKGS
else
  # Desktop: Chromium ist meistens schon dabei; sicherheitshalber nachziehen.
  # Bookworm-Desktop nennt es chromium-browser, neuere Trixie nur chromium.
  sudo apt-get install -y $COMMON_PKGS chromium-browser 2>/dev/null \
    || sudo apt-get install -y $COMMON_PKGS chromium
fi
BROWSER="$(command -v chromium-browser || command -v chromium || true)"
[ -z "$BROWSER" ] && { echo "FEHLER: Chromium konnte nicht installiert werden."; exit 1; }
echo "→ Browser: $BROWSER"

# ─── 2b) Übersetzen-Leiste abschalten ────────────────────────────────────
# Chromium hält die Monitor-Seite wegen der vielen Namen gern für englisch
# und blendet oben „Deutsch / Englisch" ein. Die Startschalter
# (--disable-features=Translate) wirken je nach Chromium-Version nicht mehr;
# eine verwaltete Richtlinie gilt immer. Beide Verzeichnisse, weil Bookworm
# (chromium-browser) und Trixie (chromium) verschieden suchen.
for POLICY_DIR in /etc/chromium/policies/managed /etc/chromium-browser/policies/managed; do
  sudo mkdir -p "$POLICY_DIR"
  echo '{ "TranslateEnabled": false }' | sudo tee "$POLICY_DIR/bts-monitor.json" >/dev/null
done
echo "→ Übersetzen-Leiste per Richtlinie abgeschaltet"

# ─── 3) Bildschirm-Abschaltung deaktivieren ──────────────────────────────
sudo raspi-config nonint do_blanking 1 || true

# ─── 3b) HDMI immer aktiv erzwingen ──────────────────────────────────────
# Ohne das wird der Pi am TV-HDMI nicht erkannt, wenn Pi und TV GLEICHZEITIG
# Strom bekommen (Pi liest beim Boot „kein Display", gibt kein Signal aus →
# kein Bild bis Pi-Neustart; Feld-Beobachtung 2026-06-14). hdmi_force_hotplug=1
# → der Pi gibt IMMER ein HDMI-Signal aus, unabhängig vom TV-Zustand beim Boot.
CONFIG_TXT="$BOOT_DIR/config.txt"
if [ -f "$CONFIG_TXT" ] && ! grep -qE "^\s*hdmi_force_hotplug=1" "$CONFIG_TXT"; then
  echo "hdmi_force_hotplug=1" | sudo tee -a "$CONFIG_TXT" >/dev/null
  echo "→ hdmi_force_hotplug=1 in $CONFIG_TXT gesetzt (Bild auch ohne TV beim Boot)"
fi
# Auf aktuellem Pi OS (KMS-Grafiktreiber vc4) wirkt hdmi_force_hotplug nicht
# mehr – dort übernimmt der Kernel-Parameter vc4.force_hotplug (Bitmaske,
# 1 = erster HDMI-Anschluss; Zero 2 W/3B+ haben nur diesen, Pi 4/5 am
# linken Anschluss ebenso). Beides
# zusammen deckt alte und neue Images ab. cmdline.txt MUSS einzeilig bleiben.
CMDLINE_TXT="$BOOT_DIR/cmdline.txt"
if [ -f "$CMDLINE_TXT" ] && ! grep -q "vc4.force_hotplug=" "$CMDLINE_TXT"; then
  sudo sed -i '1 s/[[:space:]]*$/ vc4.force_hotplug=1/' "$CMDLINE_TXT"
  echo "→ vc4.force_hotplug=1 in $CMDLINE_TXT gesetzt (KMS-Gegenstück)"
fi

# ─── 3c) TV per HDMI-CEC einschalten und auf den Pi-Eingang holen ────────
# Ziel: Steckdosenleiste an → TV und Pi bekommen gleichzeitig Strom → der
# TV zeigt ohne Fernbedienung die Monitor-Seite. Der Pi bootet langsamer
# als der TV; deshalb weckt ein kleiner Dienst den TV nach dem Start
# wiederholt (bis er „an" meldet) und meldet sich erneut als aktiver
# Eingang, wenn der TV später aus dem Standby kommt. Nur am Übergang
# Standby → an – wer bewusst auf einen anderen Eingang schaltet, wird
# nicht zurückgeholt. Abschalten je Karte: Datei bts-cec-aus.txt auf der
# Boot-Partition anlegen. Läuft auf Pi Zero 2 W, 3B+, 4, 5 (/dev/cec0).
sudo tee /usr/local/bin/bts-cec.sh >/dev/null <<'CEC'
#!/usr/bin/env bash
# bts-light Court-Monitor: TV per HDMI-CEC wecken und auf den Pi-Eingang holen.
set -u
DEV=/dev/cec0
BOOT_DIR="/boot/firmware"; [ -d "$BOOT_DIR" ] || BOOT_DIR="/boot"
# Startphase: so lange wird ein noch nicht eingeschalteter TV geweckt.
START_FENSTER_S=180
TAKT_S=5

log() { echo "bts-cec: $*"; }

if [ -e "$BOOT_DIR/bts-cec-aus.txt" ]; then
  log "abgeschaltet (bts-cec-aus.txt vorhanden)"; exit 0
fi
command -v cec-ctl >/dev/null || { log "cec-ctl fehlt (Paket v4l-utils)"; exit 0; }

# Auf das CEC-Gerät warten (Treiber lädt evtl. nach dem Dienst).
for _ in $(seq 1 60); do [ -e "$DEV" ] && break; sleep 2; done
[ -e "$DEV" ] || { log "$DEV fehlt – kein CEC an diesem Pi/Image"; exit 0; }

# Als Abspielgerät anmelden; der Kernel meldet sich selbst neu an, sobald
# der TV seine Adresse (über HDMI) bekannt gibt.
cec-ctl -d "$DEV" -s --playback -o "BTS Monitor" >/dev/null 2>&1 \
  || log "Anmelden als Abspielgerät fehlgeschlagen (TV noch ohne Strom?)"

# Physische Adresse dieses Eingangs, z. B. 1.0.0.0 (f.f.f.f = unbekannt).
# Bewusst ohne -s: die Adresse steht im Info-Block, den -s weglässt.
phys() { cec-ctl -d "$DEV" 2>/dev/null | awk -F': *' '/Physical Address/{print $2; exit}'; }
# Leistungszustand des TVs: on | standby | to-on | to-standby | leer (keine Antwort).
tv_zustand() {
  cec-ctl -d "$DEV" -s --to 0 --give-device-power-status 2>/dev/null \
    | awk '/pwr-state:/{print $2; exit}'
}
# TV einschalten und diesen Eingang als aktiv melden.
holen() {
  local pa wecken="fehlgeschlagen" eingang="nicht gesendet"
  cec-ctl -d "$DEV" -s --to 0 --image-view-on >/dev/null 2>&1 && wecken="ok"
  pa="$(phys)"
  [ "$pa" = "f.f.f.f" ] && pa=""
  if [ -n "$pa" ]; then
    # Active Source ist eine Rundsendung (an 15); Rückfall: direkt an den TV.
    eingang="fehlgeschlagen"
    { cec-ctl -d "$DEV" -s --to 15 --active-source "phys-addr=$pa" >/dev/null 2>&1 \
      || cec-ctl -d "$DEV" -s --to 0 --active-source "phys-addr=$pa" >/dev/null 2>&1; } \
      && eingang="ok"
  fi
  log "Grund: $1 · Wecken: $wecken · Eingang: $eingang (Adresse ${pa:-unbekannt})"
}

# Ab so vielen Sekunden ohne Antwort gilt der TV als aus (manche TVs
# schweigen im Standby). Einzelne verlorene Antworten lösen nichts aus –
# sonst würde ein bewusst umgeschalteter Eingang zurückgeholt.
STUMM_ALS_AUS_S=30

beginn=$(date +%s)
startphase=1
war_aus=0          # TV seit dem letzten „an" im Standby oder lange stumm
zuletzt_antwort=$beginn
while true; do
  jetzt=$(date +%s)
  zustand="$(tv_zustand)"
  if [ -n "$zustand" ]; then
    zuletzt_antwort=$jetzt
  elif [ $(( jetzt - zuletzt_antwort )) -ge $STUMM_ALS_AUS_S ]; then
    war_aus=1
  fi
  case "$zustand" in standby|to-standby) war_aus=1 ;; esac

  if [ "$startphase" = 1 ]; then
    holen "Start, TV: ${zustand:-keine Antwort}"
    # Startphase endet erst bei echtem „on" (dabei eben geholt) – ein TV im
    # Hochfahren („to-on") verwirft Befehle oft noch – oder nach dem Fenster.
    if [ "$zustand" = "on" ] || [ $(( jetzt - beginn )) -ge $START_FENSTER_S ]; then
      startphase=0
      war_aus=0
      log "Startphase beendet (TV: ${zustand:-keine Antwort})"
    fi
  elif { [ "$zustand" = "on" ] || [ "$zustand" = "to-on" ]; } && [ "$war_aus" = 1 ]; then
    # TV kam aus dem Standby (oder war ohne Strom) → Eingang neu melden.
    holen "TV eingeschaltet ($zustand)"
    # Bei „to-on" noch einmal versuchen, sobald der TV wirklich „on" meldet.
    [ "$zustand" = "on" ] && war_aus=0
  fi
  sleep "$TAKT_S"
done
CEC
sudo chmod 755 /usr/local/bin/bts-cec.sh

sudo tee /etc/systemd/system/bts-cec.service >/dev/null <<'UNIT'
[Unit]
Description=bts-light: TV per HDMI-CEC einschalten und auf den Pi-Eingang holen
Documentation=https://github.com/Assembler23/bts-light/blob/main/docs/pi-setup.md
After=local-fs.target

[Service]
ExecStart=/usr/local/bin/bts-cec.sh
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
sudo systemctl enable bts-cec.service >/dev/null 2>&1 || true
echo "→ CEC-Dienst eingerichtet (bts-cec.service): TV wird beim Start geweckt"

# ─── 3d) Nur-Lese-Modus: ausgebaut ───────────────────────────────────────
# Der Schreibschutz der SD-Karte (overlayfs im Arbeitsspeicher) machte im
# Feld Ärger – erst am Pi Zero 2 W (schwarzes Bild, 12.11.2026), dann auch
# am Pi 3. Die Karte bleibt deshalb immer beschreibbar. Hier nur noch die
# Reste einer älteren Einrichtung wegräumen.
sudo systemctl disable --now bts-nur-lesen.service >/dev/null 2>&1 || true
sudo rm -f /etc/systemd/system/bts-nur-lesen.service /usr/local/bin/bts-nur-lesen.sh \
  "$BOOT_DIR/bts-schreiben-erlaubt.txt" "$BOOT_DIR/bts-nur-lesen-versuche.txt"
sudo systemctl daemon-reload
# Ein schon vorgemerkter Schreibschutz (Eintrag gesetzt, Neustart stand noch
# aus) darf nicht mit ins Master-Image wandern.
sudo sed -i -E '1 s/[[:space:]]+(overlayroot=[^[:space:]]*|boot=overlay)//g; 1 s/^(overlayroot=[^[:space:]]*|boot=overlay)[[:space:]]+//' "$BOOT_DIR/cmdline.txt" 2>/dev/null || true
if [ -f /etc/overlayroot.conf ]; then
  sudo sed -i -E 's/^overlayroot=.*/overlayroot=""/' /etc/overlayroot.conf || true
fi

# ─── 4) Hinweisseite „noch keine Adresse" ────────────────────────────────
ASSET_DIR="$HOME/.local/share/bts-monitor"
mkdir -p "$ASSET_DIR"
cat > "$ASSET_DIR/no-url.html" <<'HTML'
<!doctype html><html lang="de" translate="no"><head><meta charset="utf-8">
<meta name="google" content="notranslate">
<title>Court-Monitor</title><style>
html,body{margin:0;height:100%;background:#0b1120;color:#f8fafc;cursor:none;
font-family:system-ui,sans-serif}
.box{height:100%;display:flex;flex-direction:column;align-items:center;
justify-content:center;gap:3vh;text-align:center;padding:6vmin}
h1{font-size:6vmin;color:#fbbf24;margin:0}
p{font-size:3.2vmin;color:#94a3b8;max-width:80vw;line-height:1.5;margin:0}
code{background:#1e293b;color:#fbbf24;padding:.1em .4em;border-radius:.3em}
</style></head><body><div class="box">
<h1>Court-Monitor – noch nicht eingerichtet</h1>
<p>Auf dieser SD-Karte ist noch keine Monitor-Adresse hinterlegt.
Stecke die Karte in einen Computer und trage die Adresse in die Datei
<code>bts-monitor-url.txt</code> ein (sie liegt direkt auf dem Karten-
Laufwerk). Die Adresse steht in bts-light unter „Court-Monitore".</p>
</div></body></html>
HTML

# ─── 5) Kiosk-Startskript ────────────────────────────────────────────────
# Dauerschleife statt Einmal-Start (übernommen aus dem früheren gemeinsamen
# Startskript, im Feld erprobt): sucht den Turnier-PC, startet den Kiosk,
# startet ihn neu, wenn Chromium abstürzt oder der PC eine neue Adresse hat,
# und sucht weiter, solange kein PC da ist („erst Pi, dann PC" ist egal).
mkdir -p "$HOME/.local/bin"
# Kopf mit den Werten aus der Einrichtung, Rumpf wörtlich (ohne Expansion).
cat > "$HOME/.local/bin/bts-monitor.sh" <<EOF
#!/usr/bin/env bash
# Court-Monitor-Kiosk – aufgerufen vom Autostart (eingerichtet von setup-monitor.sh).
BROWSER="$BROWSER"
NO_URL_PAGE="file://$ASSET_DIR/no-url.html"
EOF
cat >> "$HOME/.local/bin/bts-monitor.sh" <<'EOF'
set -u
LOG="${XDG_RUNTIME_DIR:-/tmp}/bts-monitor.log"
CACHE="${XDG_RUNTIME_DIR:-/tmp}/bts-monitor-ip"
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
    echo "http://$ip:$port${PFAD}${sep}device=${DEVICE_ID}"
  else
    # Feste Adresse (IP oder Cloud): so nehmen, wie sie in der Datei steht.
    erreichbar "$URL" || return 1
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
EOF
chmod +x "$HOME/.local/bin/bts-monitor.sh"

# ─── 6) Variant-spezifischer Autostart ───────────────────────────────────
if [ "$VARIANT" = "desktop" ]; then
  # LXDE/openbox-Autostart per .desktop-Datei.
  mkdir -p "$HOME/.config/autostart"
  cat > "$HOME/.config/autostart/bts-monitor.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=bts-light Court-Monitor
Exec=$HOME/.local/bin/bts-monitor.sh
X-GNOME-Autostart-enabled=true
EOF
  echo "→ Desktop-Autostart geschrieben: ~/.config/autostart/bts-monitor.desktop"
else
  # Lite: kein Session-Manager → wir bauen den X-Start selbst.
  # 6a) Konsole-Autologin: badhub loggt sich auf tty1 automatisch ein.
  sudo raspi-config nonint do_boot_behaviour B2 || true
  echo "→ Console-Autologin auf tty1 eingerichtet (raspi-config B2)"

  # 6b) .xinitrc startet den minimalen WM + Kiosk-Skript.
  cat > "$HOME/.xinitrc" <<XINIT
#!/bin/sh
# bts-light Court-Monitor: X-Session, gestartet via 'startx' aus .bash_profile.
# Kein DPMS, kein Screensaver — TV soll dauerhaft anzeigen.
xset -dpms
xset s off
xset s noblank
# Minimaler Window-Manager (sonst rendert Chromium nicht zuverlaessig fullscreen).
matchbox-window-manager -use_titlebar no &
# Kiosk starten.
exec "\$HOME/.local/bin/bts-monitor.sh"
XINIT
  chmod +x "$HOME/.xinitrc"
  echo "→ ~/.xinitrc geschrieben"

  # 6c) .bash_profile-Hook: bei tty1-Login automatisch 'startx'.
  touch "$HOME/.bash_profile"
  if ! grep -q "bts-light Court-Monitor" "$HOME/.bash_profile" 2>/dev/null; then
    cat >> "$HOME/.bash_profile" <<'PROFILE'

# bts-light Court-Monitor: bei tty1-Login automatisch X-Session starten.
# Nur tty1, nicht für SSH oder andere ttys (sonst loopen wir).
if [ -z "${DISPLAY:-}" ] && [ "$(tty)" = "/dev/tty1" ]; then
  exec startx
fi
PROFILE
    echo "→ ~/.bash_profile-Hook angehängt"
  else
    echo "→ ~/.bash_profile-Hook war schon drin, übersprungen"
  fi
fi

echo
echo "✓ Fertig eingerichtet (Variante: $VARIANT)."
echo
echo "  Adresse setzen/ändern: $URL_FILE bearbeiten"
echo "  (geht auch am Computer – die Datei liegt auf dem Karten-Laufwerk)."
echo
echo "  Jetzt neu starten:  sudo reboot"
if [ "$VARIANT" = "lite" ]; then
  echo "  Kiosk beenden:      Tastatur anschließen, Strg+Alt+F2 → andere tty"
else
  echo "  Kiosk beenden:      Tastatur anschließen, Alt+F4"
fi
