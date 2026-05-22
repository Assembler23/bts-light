#!/usr/bin/env bash
#
# bts-light Court-Monitor – Kiosk-Einrichtung für den Raspberry Pi.
#
# Verwandelt ein frisches „Raspberry Pi OS (Desktop)" in eine Vollbild-
# Court-Monitor-Anzeige. Einmal ausführen, danach neu starten.
#
# Die Monitor-Adresse wird NICHT fest eingebaut, sondern bei jedem Start
# aus der Datei  bts-monitor-url.txt  auf der Boot-Partition der SD-Karte
# gelesen. Dadurch eignet sich eine eingerichtete Karte als Master-Image:
# einmal klonen – jeder Betreiber trägt nur seine Adresse in diese eine
# Datei ein (von jedem Computer aus mit einem Texteditor editierbar).
#
# Anleitung: docs/pi-setup.md · Master-Image: docs/pi-master-image.md

set -euo pipefail

echo "── bts-light Court-Monitor · Pi-Einrichtung ──────────────────────"

# Boot-Partition finden (Raspberry Pi OS Bookworm: /boot/firmware, älter: /boot).
BOOT_DIR="/boot/firmware"
[ -d "$BOOT_DIR" ] || BOOT_DIR="/boot"
URL_FILE="$BOOT_DIR/bts-monitor-url.txt"

# 1) Monitor-Adresse ──────────────────────────────────────────────────────
# Enter = Standardadresse über den festen Namen `bts-light.local`. Die
# funktioniert dank mDNS in JEDEM Turnier-WLAN, ohne feste IP – ideal
# fürs Master-Image. Eine eigene Adresse lässt sich später jederzeit in
# bts-monitor-url.txt eintragen.
DEFAULT_URL="http://bts-light.local:8088/monitor"
echo
echo 'Monitor-Adresse (steht in bts-light unter „Court-Monitore").'
echo "Enter = Standard  $DEFAULT_URL"
echo "(passt für jedes Turnier – ideal fürs Master-Image)."
read -rp "Adresse: " URL || true
[ -z "${URL:-}" ] && URL="$DEFAULT_URL"
echo "$URL" | sudo tee "$URL_FILE" >/dev/null
echo "→ gespeichert in $URL_FILE: $URL"

# 2) Chromium sicherstellen ───────────────────────────────────────────────
BROWSER="$(command -v chromium-browser || command -v chromium || true)"
if [ -z "$BROWSER" ]; then
  echo "Chromium nicht gefunden – wird installiert …"
  sudo apt-get update
  sudo apt-get install -y chromium-browser || sudo apt-get install -y chromium
  BROWSER="$(command -v chromium-browser || command -v chromium)"
fi
echo "→ Browser: $BROWSER"

# 3) unclutter – versteckt den Mauszeiger (nur X11) ───────────────────────
sudo apt-get install -y unclutter 2>/dev/null \
  || echo "Hinweis: unclutter nicht installiert (unkritisch)."

# 4) Bildschirm-Abschaltung deaktivieren ─────────────────────────────────
sudo raspi-config nonint do_blanking 1 || true

# 5) Hinweisseite „noch keine Adresse" ───────────────────────────────────
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

# 6) Start-Skript schreiben ───────────────────────────────────────────────
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/bts-monitor.sh" <<EOF
#!/usr/bin/env bash
# Court-Monitor-Kiosk – aufgerufen vom Autostart. Liest die Adresse bei
# jedem Start frisch aus der Boot-Partition.
set -u

# Monitor-Adresse aus der Boot-Partition (überall white-space entfernen,
# damit ein Zeilenumbruch aus dem Texteditor nicht stört).
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
[ -f "\$PREF" ] && sed -i \
  's/"exited_cleanly":false/"exited_cleanly":true/;s/"exit_type":"[^"]*"/"exit_type":"Normal"/' \
  "\$PREF" 2>/dev/null || true

command -v unclutter >/dev/null && unclutter -idle 0.5 -root &

exec "$BROWSER" \\
  --kiosk --incognito --noerrdialogs --disable-infobars \\
  --disable-session-crashed-bubble --no-first-run \\
  --check-for-update-interval=31536000 \\
  "\$TARGET"
EOF
chmod +x "$HOME/.local/bin/bts-monitor.sh"

# 7) Autostart-Verknüpfung ────────────────────────────────────────────────
mkdir -p "$HOME/.config/autostart"
cat > "$HOME/.config/autostart/bts-monitor.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=bts-light Court-Monitor
Exec=$HOME/.local/bin/bts-monitor.sh
X-GNOME-Autostart-enabled=true
EOF

echo
echo "✓ Fertig eingerichtet."
echo
echo "  Adresse setzen/ändern: $URL_FILE bearbeiten"
echo "  (geht auch am Computer – die Datei liegt auf dem Karten-Laufwerk)."
echo
echo "  Jetzt neu starten:  sudo reboot"
echo "  Kiosk beenden:      Tastatur anschließen, Alt+F4"
