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
# Kiosk-Skript, CEC-Dienst und Hinweisseite stehen nicht mehr in diesem
# Skript, sondern als echte Dateien unter pi/paket/ (Liste: pi/paket/INHALT).
# Liegt der Ordner nicht daneben (Skript einzeln geladen), holt es ihn aus
# dem Repository.
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

# ─── Paketdateien finden ─────────────────────────────────────────────────
# Die Dateien, die auf dem Pi landen, liegen unter pi/paket/ – dieselbe Quelle
# für das Image und (später) für das Fernupdate. Wer nur dieses Skript geladen
# hat (Anleitung docs/pi-setup.md), bekommt den Ordner aus dem Repository;
# BTS_REPO_REF wählt Zweig oder Tag (Standard: main).
SKRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
PAKET_DIR="${SKRIPT_DIR:+$SKRIPT_DIR/paket}"
if [ -z "$PAKET_DIR" ] || [ ! -f "$PAKET_DIR/INHALT" ]; then
  PAKET_REF="${BTS_REPO_REF:-main}"
  PAKET_TMP="$(mktemp -d)"
  trap 'rm -rf "$PAKET_TMP"' EXIT
  command -v curl >/dev/null 2>&1 && command -v tar >/dev/null 2>&1 \
    || { echo "FEHLER: curl und tar werden zum Laden der Paketdateien gebraucht (oder den Ordner paket/ neben dieses Skript legen)."; exit 1; }
  echo "→ Paketdateien laden (Assembler23/bts-light, $PAKET_REF)…"
  curl -fsSL "https://github.com/Assembler23/bts-light/archive/$PAKET_REF.tar.gz" \
    | tar -xz -C "$PAKET_TMP" --strip-components=2 \
        --wildcards --no-wildcards-match-slash '*/pi/paket' \
    || { echo "FEHLER: Paketdateien konnten nicht geladen werden (Internet? Zweig: $PAKET_REF)."; exit 1; }
  PAKET_DIR="$PAKET_TMP/paket"
fi
# Vollständigkeit vor der ersten Änderung am System prüfen – ein halb
# geladener Ordner soll nicht mitten in der Einrichtung auffallen.
[ -f "$PAKET_DIR/INHALT" ] || { echo "FEHLER: Paketdateien fehlen ($PAKET_DIR)."; exit 1; }
while read -r _modus datei; do
  case "$_modus" in ''|'#'*) continue ;; esac
  [ -f "$PAKET_DIR/$datei" ] || { echo "FEHLER: Paketdatei fehlt: $datei"; exit 1; }
done < "$PAKET_DIR/INHALT"
echo "→ Paketdateien: $PAKET_DIR"

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
sudo install -m 755 "$PAKET_DIR/bin/bts-cec.sh" /usr/local/bin/bts-cec.sh

sudo install -m 644 "$PAKET_DIR/units/bts-cec.service" /etc/systemd/system/bts-cec.service
sudo systemctl daemon-reload
sudo systemctl enable bts-cec.service >/dev/null 2>&1 || true
echo "→ CEC-Dienst eingerichtet (bts-cec.service): TV wird beim Start geweckt, bts-light kann ihn schalten"

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
install -m 644 "$PAKET_DIR/share/no-url.html" "$ASSET_DIR/no-url.html"

# ─── 5) Kiosk-Startskript ────────────────────────────────────────────────
# Dauerschleife statt Einmal-Start (übernommen aus dem früheren gemeinsamen
# Startskript, im Feld erprobt): sucht den Turnier-PC, startet den Kiosk,
# startet ihn neu, wenn Chromium abstürzt oder der PC eine neue Adresse hat,
# und sucht weiter, solange kein PC da ist („erst Pi, dann PC" ist egal).
mkdir -p "$HOME/.local/bin"
install -m 755 "$PAKET_DIR/bin/bts-monitor.sh" "$HOME/.local/bin/bts-monitor.sh"

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
  install -m 755 "$PAKET_DIR/xinitrc" "$HOME/.xinitrc"
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
