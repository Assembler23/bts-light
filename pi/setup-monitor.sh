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
COMMON_PKGS="unclutter fonts-noto-color-emoji"
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

# ─── 3) Bildschirm-Abschaltung deaktivieren ──────────────────────────────
sudo raspi-config nonint do_blanking 1 || true

# ─── 4) Hinweisseite „noch keine Adresse" ────────────────────────────────
ASSET_DIR="$HOME/.local/share/bts-monitor"
mkdir -p "$ASSET_DIR"
cat > "$ASSET_DIR/no-url.html" <<'HTML'
<!doctype html><html lang="de"><head><meta charset="utf-8">
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
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/bts-monitor.sh" <<EOF
#!/usr/bin/env bash
# Court-Monitor-Kiosk – aufgerufen vom Autostart. Liest die Adresse bei
# jedem Start frisch aus der Boot-Partition.
set -u

# Monitor-Adresse aus der Boot-Partition (whitespace tolerieren).
BOOT_DIR="/boot/firmware"; [ -d "\$BOOT_DIR" ] || BOOT_DIR="/boot"
URL="\$(tr -d '[:space:]' < "\$BOOT_DIR/bts-monitor-url.txt" 2>/dev/null || true)"

# Geräte-ID aus der Hardware-Seriennummer (letzte 8 Stellen). Frisch bei
# jedem Start gelesen → eine geklonte Karte bleibt je Pi eindeutig.
SERIAL="\$(tr -d '\\0' < /sys/firmware/devicetree/base/serial-number 2>/dev/null || true)"
[ -z "\$SERIAL" ] && SERIAL="\$(awk '/^Serial/{print \$NF}' /proc/cpuinfo 2>/dev/null || true)"
[ -z "\$SERIAL" ] && SERIAL="\$(hostname)"
DEVICE="\$(printf '%s' "\$SERIAL" | tail -c 8)"

if [ -n "\$URL" ]; then
  SEP="?"; case "\$URL" in *\\?*) SEP="&" ;; esac
  TARGET="\${URL}\${SEP}device=\${DEVICE}"
else
  # Keine Adresse hinterlegt → Hinweisseite.
  TARGET="file://$ASSET_DIR/no-url.html"
fi

# Nach einem Stromausfall Chromiums „Wiederherstellen?"-Frage unterdrücken.
PREF="\$HOME/.config/chromium/Default/Preferences"
[ -f "\$PREF" ] && sed -i \\
  's/"exited_cleanly":false/"exited_cleanly":true/;s/"exit_type":"[^"]*"/"exit_type":"Normal"/' \\
  "\$PREF" 2>/dev/null || true

command -v unclutter >/dev/null && unclutter -idle 0.5 -root &

exec "$BROWSER" --no-memcheck \\
  --kiosk --incognito --noerrdialogs --disable-infobars \\
  --disable-session-crashed-bubble --no-first-run \\
  --check-for-update-interval=31536000 \\
  "\$TARGET"
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
