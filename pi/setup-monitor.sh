#!/usr/bin/env bash
#
# bts-light Court-Monitor – Kiosk-Einrichtung für den Raspberry Pi.
#
# Macht aus einem frisch installierten „Raspberry Pi OS (Desktop)" eine
# Vollbild-Anzeige, die beim Einschalten automatisch den Court-Monitor
# zeigt. Einmal ausführen, danach neu starten.
#
# Aufruf:  bash setup-monitor.sh
#     oder: bash setup-monitor.sh "http://192.168.1.50:8088/monitor"
#
# Anleitung: docs/pi-setup.md im bts-light-Repo.

set -euo pipefail

echo "── bts-light Court-Monitor · Pi-Einrichtung ──────────────────────"

# 1) Monitor-Adresse bestimmen ────────────────────────────────────────────
# Alle Monitore eines Turniers nutzen dieselbe Adresse – sie steht in
# bts-light unter „Court-Monitore".
URL="${1:-}"
while [ -z "$URL" ]; do
  echo
  echo 'Die Monitor-Adresse steht in bts-light unter „Court-Monitore".'
  read -rp "Monitor-Adresse eingeben: " URL
done
echo "→ Adresse: $URL"

# 2) Chromium sicherstellen ───────────────────────────────────────────────
# Auf dem Desktop-Image ist Chromium bereits dabei; falls nicht, nachholen.
BROWSER="$(command -v chromium-browser || command -v chromium || true)"
if [ -z "$BROWSER" ]; then
  echo "Chromium nicht gefunden – wird installiert …"
  sudo apt-get update
  sudo apt-get install -y chromium-browser || sudo apt-get install -y chromium
  BROWSER="$(command -v chromium-browser || command -v chromium)"
fi
echo "→ Browser: $BROWSER"

# 3) unclutter – versteckt den Mauszeiger (nur X11; auf Wayland ohne Wirkung)
sudo apt-get install -y unclutter 2>/dev/null \
  || echo "Hinweis: unclutter nicht installiert – der Mauszeiger bleibt evtl. sichtbar."

# 4) Bildschirm-Abschaltung deaktivieren ──────────────────────────────────
# Ohne das wird der TV nach einigen Minuten schwarz.
sudo raspi-config nonint do_blanking 1 || true

# 5) Start-Skript schreiben ───────────────────────────────────────────────
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/bts-monitor.sh" <<EOF
#!/usr/bin/env bash
# Startet den Court-Monitor im Vollbild – aufgerufen vom Autostart.
URL="$URL"

# Falls der Pi den Strom verloren hat: Chromiums „Wiederherstellen?"-
# Frage unterdrücken, indem der Absturz-Marker zurückgesetzt wird.
PREF="\$HOME/.config/chromium/Default/Preferences"
[ -f "\$PREF" ] && sed -i \
  's/"exited_cleanly":false/"exited_cleanly":true/;s/"exit_type":"[^"]*"/"exit_type":"Normal"/' \
  "\$PREF" 2>/dev/null || true

# Mauszeiger ausblenden (nur X11).
command -v unclutter >/dev/null && unclutter -idle 0.5 -root &

exec "$BROWSER" \\
  --kiosk --incognito --noerrdialogs --disable-infobars \\
  --disable-session-crashed-bubble --no-first-run \\
  --check-for-update-interval=31536000 \\
  "\$URL"
EOF
chmod +x "$HOME/.local/bin/bts-monitor.sh"

# 6) Autostart-Verknüpfung ────────────────────────────────────────────────
# ~/.config/autostart wird vom Pi-Desktop beim Login automatisch gestartet.
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
echo "  Jetzt neu starten:   sudo reboot"
echo "  Danach öffnet sich der Court-Monitor automatisch im Vollbild."
echo
echo "  Adresse später ändern: dieses Skript erneut ausführen."
echo "  Kiosk beenden:         Tastatur anschließen, Alt+F4 drücken."
