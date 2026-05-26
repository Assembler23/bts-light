#!/usr/bin/env bash
#
# bts-light Court-Monitor — Master-Image-Builder
#
# Wird auf einem fertig eingerichteten Master-Pi ausgeführt, BEVOR die
# SD-Karte als `.img` geklont wird. Räumt alle gerätespezifischen Spuren
# weg, damit der Klon auf einer fremden Karte wieder wie ein frisches
# Pi-OS aussieht — mit dem Unterschied, dass Kiosk, Chromium, X11 und
# `bts-monitor.sh` schon installiert sind.
#
# Was passiert hier:
#   1) Persistierender Hook: bei jedem Boot wird geprüft, ob SSH-Hostkeys
#      da sind. Falls nicht → automatisch neu generieren (= Stolperstein 2
#      aus docs/pi-master-image.md).
#   2) Identität entfernen: Hostname, WLAN-Config, SSH-authorized_keys,
#      machine-id (= Stolperstein 3). Pi Imagers Custom-Options dürfen
#      beim Schreiben auf eine neue Karte diese Werte wieder setzen.
#   3) Caches + Logs leeren — sonst hat das Klon-Image unnötig Müll und
#      verrät die letzte Master-Pi-Aktivität.
#   4) Sync + Hinweis zum Shutdown.
#
# Aufruf (auf dem Master-Pi):
#   sudo bash build-master-image.sh
#
# Danach:
#   sudo shutdown -h now
#   → SD-Karte raus, in Mac/Linux-Host, mit `dd` das Image ziehen.

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Bitte mit sudo aufrufen: sudo bash $0" >&2
  exit 1
fi

USER_HOME="/home/badhub"
MASTER_HOSTNAME="bts-monitor"

echo "── bts-light Master-Image-Builder ──────────────────────────────"
echo "Master-Pi wird auf 'image-ready' vorbereitet."
echo

# ─── 1) SSH-Hostkey-Regenerator als persistente Unit ────────────────────
# Pi-OS hat zwar einen ähnlichen Service `regenerate_ssh_host_keys`, der
# disabled sich aber nach dem ersten Boot selbst. Wir legen eine eigene,
# idempotente Unit an, die bei jedem Boot prüft und nur ausführt, wenn
# tatsächlich keine Hostkeys da sind.
echo "→ SSH-Hostkey-Regenerator-Service einrichten…"
cat > /etc/systemd/system/bts-regen-ssh-hostkeys.service <<'UNIT'
[Unit]
Description=bts-light: Regenerate SSH host keys if missing (Master-Image)
Documentation=https://github.com/Assembler23/bts-light/blob/main/docs/pi-master-image.md
Before=ssh.service ssh.socket
After=local-fs.target
ConditionPathExistsGlob=!/etc/ssh/ssh_host_*_key

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/bin/ssh-keygen -A

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable bts-regen-ssh-hostkeys.service
echo "  ✓ bts-regen-ssh-hostkeys.service enabled"

# ─── 2) Identität entfernen ─────────────────────────────────────────────
echo "→ Hostname auf '$MASTER_HOSTNAME' zurücksetzen…"
echo "$MASTER_HOSTNAME" > /etc/hostname
# /etc/hosts: 127.0.1.1-Eintrag ist Hostname-spezifisch. Wir patchen ihn,
# auch wenn er gerade einen anderen Wert trägt.
if grep -q '^127\.0\.1\.1' /etc/hosts; then
  sed -i "s/^127\.0\.1\.1.*/127.0.1.1\t$MASTER_HOSTNAME/" /etc/hosts
else
  echo -e "127.0.1.1\t$MASTER_HOSTNAME" >> /etc/hosts
fi
echo "  ✓ /etc/hostname + /etc/hosts"

echo "→ WLAN-Konfiguration BEIBEHALTEN (Plug-and-Play-Image)…"
# Bewusste Entscheidung: Die WLAN-Daten aus der Master-Pi-Konfiguration
# bleiben im Image. Damit ist das Ergebnis ein **echtes** Plug-and-Play-
# Image für das Verleih-Set: SD-Karte schreiben, in den Pi stecken, der
# bootet direkt ins „Turnier"-WLAN — kein Eintippen, kein Pi-Imager-
# Custom-Options-Dialog nötig. Voraussetzung ist eine einheitliche
# Router-SSID (siehe docs/pi-master-image.md Teil A).
#
# Wer das Image fuer einen Fremd-Verein zur Verfuegung stellen will,
# soll vor dem dd selbst entscheiden, ob die WLAN-Daten im Image
# bleiben oder weg (in dem Fall: WPA_CONF entsprechend leeren).
echo "  ✓ /etc/wpa_supplicant/wpa_supplicant.conf bleibt unangetastet"

echo "→ SSH-authorized_keys leeren (Image-Hygiene; SSH kann via Pi-Imager-Pubkey-Dialog oder ssh-copy-id pro Pi wieder hinzu)…"
for hk in "$USER_HOME/.ssh/authorized_keys" "/root/.ssh/authorized_keys"; do
  if [ -f "$hk" ]; then
    : > "$hk"
    echo "  ✓ $hk geleert"
  fi
done

echo "→ SSH-Hostkeys + machine-id loeschen (werden bei Boot neu generiert)…"
rm -f /etc/ssh/ssh_host_*
echo "  ✓ /etc/ssh/ssh_host_* entfernt"
: > /etc/machine-id
rm -f /var/lib/dbus/machine-id
echo "  ✓ /etc/machine-id geleert, /var/lib/dbus/machine-id entfernt"

# ─── 3) Caches + Logs leeren ────────────────────────────────────────────
echo "→ Caches und Logs leeren…"
journalctl --rotate >/dev/null 2>&1 || true
journalctl --vacuum-time=1s >/dev/null 2>&1 || true
apt-get clean >/dev/null 2>&1 || true
rm -f "$USER_HOME/.bash_history" /root/.bash_history \
      "$USER_HOME/.lesshst" /root/.lesshst \
      "$USER_HOME/.python_history" /root/.python_history
rm -rf /tmp/*.log /tmp/.X*-lock
echo "  ✓ journal, apt-cache, history"

# cloud-init nur aufräumen, wenn vorhanden — Pi-OS Lite hat es manchmal.
if command -v cloud-init >/dev/null 2>&1; then
  cloud-init clean --logs --seed >/dev/null 2>&1 || true
  echo "  ✓ cloud-init clean"
fi

# ─── 4) Sync + Hinweis ──────────────────────────────────────────────────
sync
sleep 2
sync

cat <<EOM

✓ Master-Pi ist 'image-ready'.

Jetzt:
  sudo shutdown -h now

Wenn die grüne LED dauerhaft aus ist:
  → SD-Karte raus, in einen Host-Computer (Mac/Linux).
  → Mit dd das Image ziehen:
       sudo dd if=/dev/<karte> of=\$HOME/bts-monitor-master.img bs=4M
  → Image schrumpfen (optional, mit pishrink) und .xz-komprimieren.
  → Auf badhub.de hochladen oder als GitHub-Release veroeffentlichen.

Weitere Pis im Verleih-Set bekommen das Image dann via Raspberry Pi
Imager ('Choose OS' → 'Use Custom') zusammen mit ihren eigenen
Hostname/WLAN/SSH-Custom-Options. Beim ersten Boot greift dann der
firstrun.sh-Hook des Pi-Imagers; SSH-Hostkeys werden automatisch
generiert (bts-regen-ssh-hostkeys.service).
EOM
