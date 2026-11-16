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

# ─── Fernschalten durch die Turnierleitung ───────────────────────────────
# bts-light kann den TV an- und ausschalten (Seite „Court-Monitore"). Der
# Browser kann kein CEC, deshalb fragt dieser Dienst selbst beim Turnier-PC
# nach: Er meldet den Zustand des TVs und bekommt „an <Marke>", „aus <Marke>"
# oder „-" zurück. Ausgeführt wird ein Befehl genau einmal, wenn sich die
# Marke ändert – wer danach zur Fernbedienung greift, wird nicht überstimmt.
# Die Adresse des PCs legt das Kiosk-Skript ab (nur im Hallennetz; bei einer
# Cloud-Adresse gibt es die Datei nicht, dann wird nichts abgefragt).
SERIAL="$(awk -F': ' '/^Serial/{print $2}' /proc/cpuinfo 2>/dev/null | tr -d '[:space:]')"
[ -z "$SERIAL" ] && SERIAL="$(tr -d '\0' 2>/dev/null < /sys/firmware/devicetree/base/serial-number || true)"
[ -z "$SERIAL" ] && SERIAL="$(hostname | tr -d '[:space:]')"
GERAET="pi-${SERIAL}"
# Nur eine echte Datei lesen (kein Verweis, keine Pipe): Dieser Dienst läuft
# als root, die Datei schreibt der Kiosk-Benutzer – eine dort abgelegte Pipe
# würde das Lesen sonst für immer anhalten.
pc_basis() {
  local f
  for f in /run/user/*/bts-monitor-pc /tmp/bts-monitor-pc; do
    if [ -f "$f" ] && [ ! -L "$f" ]; then
      head -c 200 "$f" 2>/dev/null | head -1
      return 0
    fi
  done
}
ERSTE_ABFRAGE=1    # bis zur ersten Antwort: „frisch gestartet" mitmelden
LETZTE_MARKE=""
BEFEHL=""
# $1 = Zustand des TVs. Setzt BEFEHL auf an|aus, wenn ein neuer Befehl da ist.
befehl_holen() {
  BEFEHL=""
  local basis antwort soll marke
  basis="$(pc_basis)"
  printf '%s' "$basis" | grep -Eq '^http://[0-9A-Za-z.:-]+$' || return 0
  antwort=$(curl -fs --connect-timeout 1 --max-time 2 --max-filesize 64 \
    "${basis}/pi-tv?device=${GERAET}&tv=${1:-unknown}&start=${ERSTE_ABFRAGE}" 2>/dev/null) || return 0
  ERSTE_ABFRAGE=0
  read -r soll marke <<<"$antwort"
  case "$soll" in an|aus) ;; *) return 0 ;; esac
  case "$marke" in ''|*[!0-9]*) return 0 ;; esac
  [ "$marke" = "$LETZTE_MARKE" ] && return 0
  LETZTE_MARKE="$marke"
  BEFEHL="$soll"
}
ausschalten() {
  if cec-ctl -d "$DEV" -s --to 0 --standby >/dev/null 2>&1; then
    log "Grund: $1 · Ausschalten: ok"
  else
    log "Grund: $1 · Ausschalten: fehlgeschlagen"
  fi
}

# Ab so vielen Sekunden ohne Antwort gilt der TV als aus (manche TVs
# schweigen im Standby). Einzelne verlorene Antworten lösen nichts aus –
# sonst würde ein bewusst umgeschalteter Eingang zurückgeholt.
STUMM_ALS_AUS_S=30

beginn=$(date +%s)
startphase=1
war_aus=0          # TV seit dem letzten „an" im Standby oder lange stumm
zuletzt_antwort=$beginn
grund="Start"      # wer das Wecken ausgelöst hat (fürs Protokoll)
aus_rest=0         # so oft wird „aus" noch wiederholt, solange der TV „an" meldet
while true; do
  jetzt=$(date +%s)
  zustand="$(tv_zustand)"
  if [ -n "$zustand" ]; then
    zuletzt_antwort=$jetzt
  elif [ $(( jetzt - zuletzt_antwort )) -ge $STUMM_ALS_AUS_S ]; then
    war_aus=1
  fi
  case "$zustand" in standby|to-standby) war_aus=1 ;; esac

  befehl_holen "$zustand"
  if [ "$BEFEHL" = "aus" ]; then
    ausschalten "Turnierleitung"
    # Kein Wecken mehr, und der noch „an" meldende TV darf nicht als
    # „eben eingeschaltet" gelten – sonst holte ihn die Regel unten zurück.
    startphase=0
    war_aus=0
    aus_rest=3
    sleep "$TAKT_S"
    continue
  elif [ "$BEFEHL" = "an" ]; then
    # Wie beim Start: wecken und wiederholen, bis der TV „an" meldet.
    aus_rest=0
    startphase=1
    beginn=$jetzt
    grund="Turnierleitung"
  elif [ "$aus_rest" -gt 0 ]; then
    # Manche TVs überhören den ersten Standby-Befehl.
    if [ "$zustand" = "on" ]; then
      aus_rest=$((aus_rest - 1))
      ausschalten "Turnierleitung (Wiederholung)"
      sleep "$TAKT_S"
      continue
    fi
    aus_rest=0
  fi

  if [ "$startphase" = 1 ]; then
    holen "$grund, TV: ${zustand:-keine Antwort}"
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
