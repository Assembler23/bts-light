#!/usr/bin/env bash
#
# bts-light Court-Monitor — Pi-Image ohne Pi bauen (Docker + ARM-Emulation).
#
# Macht dasselbe wie die Handarbeit aus docs/pi-master-image.md Teil B:
# Raspberry Pi OS Lite (64-bit) nehmen, Benutzer badhub anlegen, WLAN
# eintragen, setup-monitor.sh und build-master-image.sh ausführen — nur in
# einem Container statt auf einem echten Pi. Ergebnis: bts-light-pi.img.xz
# (+ .sha256) im Ausgabeordner, fertig für den Download-Bereich.
#
# Das Image läuft dabei NIE auf einem Pi: nach jedem Neubau einmal auf
# echter Hardware starten, bevor es hochgeladen wird.
#
# Voraussetzung: Docker mit --privileged (Loop-Geräte, binfmt für ARM).
# Achtung: registriert qemu-aarch64 im binfmt_misc des Host-Kernels (bis
# zum Neustart).
#
# Aufruf:
#   WLAN_PSK='…' bash pi/build-image-docker.sh <ausgabeordner>
# Optional: WLAN_SSID (Standard btsaccess), PI_PASSWORT (Standard bts-light),
#           BASIS_URL (Raspberry Pi OS Lite arm64, .img.xz)

set -euo pipefail
OUT="${1:?Ausgabeordner angeben}"
: "${WLAN_PSK:?WLAN_PSK setzen (Passwort des Verleih-WLANs)}"
WLAN_SSID="${WLAN_SSID:-btsaccess}"
PI_PASSWORT="${PI_PASSWORT:-bts-light}"
BASIS_URL="${BASIS_URL:-https://downloads.raspberrypi.com/raspios_oldstable_lite_arm64_latest}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
# NetworkManager-Keyfile: Backslash/Zeilenumbruch würden die Datei zerlegen,
# „/" den Dateinamen.
case "$WLAN_PSK$WLAN_SSID" in *\\*|*$'\n'*) echo "WLAN-Daten ohne \\ und Zeilenumbruch angeben" >&2; exit 1 ;; esac
case "$WLAN_SSID" in */*) echo "WLAN_SSID darf kein / enthalten" >&2; exit 1 ;; esac
[ ${#WLAN_PSK} -ge 8 ] && [ ${#WLAN_PSK} -le 63 ] || { echo "WLAN_PSK muss 8–63 Zeichen haben" >&2; exit 1; }
export WLAN_SSID WLAN_PSK PI_PASSWORT

mkdir -p "$OUT/dl" "$OUT/out" "$OUT/pi"
OUT="$(cd "$OUT" && pwd)"
if [ ! -s "$OUT/dl/basis.img.xz" ]; then
  # _latest leitet auf die konkrete Datei weiter; deren .sha256 liegt daneben.
  ZIEL="$(curl -sSfLI -o /dev/null -w '%{url_effective}' "$BASIS_URL")"
  echo "Basis: $ZIEL"
  # Erst prüfen, dann umbenennen: ein abgebrochener Download darf beim
  # nächsten Lauf nicht ungeprüft als Basis gelten.
  curl -sSfL "$ZIEL" -o "$OUT/dl/basis.img.xz.part"
  SOLL="$(curl -sSfL "$ZIEL.sha256" | awk '{print $1}')"
  echo "$SOLL  $OUT/dl/basis.img.xz.part" | sha256sum -c
  mv "$OUT/dl/basis.img.xz.part" "$OUT/dl/basis.img.xz"
fi
cp "$REPO/pi/setup-monitor.sh" "$REPO/pi/build-master-image.sh" "$OUT/pi/"
sed -n '/^#--- INNEN ---$/,$p' "$0" | tail -n +2 > "$OUT/innen.sh"
docker run --rm --privileged -v "$OUT:/build" \
  -e WLAN_SSID -e WLAN_PSK -e PI_PASSWORT -e HOST_ID="$(id -u):$(id -g)" \
  debian:bookworm bash /build/innen.sh
echo "Fertig: $OUT/out/bts-light-pi.img.xz"
exit 0

# Alles ab hier läuft IM Container (Debian, root).
#--- INNEN ---
#!/bin/bash
set -euxo pipefail
SRC=/build/dl/basis.img.xz
IMG=/build/out/bts-light-pi.img
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq qemu-user-static binfmt-support xz-utils parted e2fsprogs fdisk zerofree util-linux >/dev/null
mountpoint -q /proc/sys/fs/binfmt_misc || mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc
update-binfmts --enable qemu-aarch64 || true
sed -n 1,3p /proc/sys/fs/binfmt_misc/qemu-aarch64

# Aufräumen nur für das EIGENE Loop-Gerät – Loop-Geräte sind kernelweit,
# ein Muster-Abgleich träfe auch fremde Geräte des Hosts.
LOOP=""
aufraeumen() {
  set +e
  for d in dev/pts dev sys proc boot/firmware ""; do umount -l "/mnt/r/$d" 2>/dev/null; done
  [ -n "$LOOP" ] && losetup -d "$LOOP"
}
trap aufraeumen EXIT
xz -dc "$SRC" > "$IMG"
truncate -s +3G "$IMG"
parted -s "$IMG" resizepart 2 100%
LOOP=$(losetup -fP --show "$IMG")
# Partitions-Knoten im Container nachziehen
for n in 1 2; do [ -e ${LOOP}p$n ] || mknod ${LOOP}p$n b $(cat /sys/block/$(basename $LOOP)/$(basename $LOOP)p$n/dev | tr ':' ' '); done
e2fsck -fy ${LOOP}p2 || true
resize2fs ${LOOP}p2
R=/mnt/r; mkdir -p $R
mount ${LOOP}p2 $R
mount ${LOOP}p1 $R/boot/firmware
for d in proc sys dev dev/pts; do mount --bind /$d $R/$d; done
cp -a $R/etc/resolv.conf /tmp/resolv.orig 2>/dev/null || true
rm -f $R/etc/resolv.conf; cp /etc/resolv.conf $R/etc/resolv.conf
cp /build/pi/setup-monitor.sh /build/pi/build-master-image.sh $R/tmp/

# Geheimnisse über eine root-only-Datei (nicht per Kommandozeile/Trace).
set +x
( umask 077; printf 'WLAN_SSID=%q\nWLAN_PSK=%q\nPI_PASSWORT=%q\n' "$WLAN_SSID" "$WLAN_PSK" "$PI_PASSWORT" > $R/root/.bts-build-env )
set -x
chroot $R /bin/bash -euxo pipefail <<'IN'
set +x; . /root/.bts-build-env; rm -f /root/.bts-build-env; set -x
echo "findmnt im chroot: $(findmnt -n -o FSTYPE /)"
# Benutzer badhub (Imager-Weg: der vorhandene pi wird umbenannt)
if id pi >/dev/null 2>&1; then
  usermod -l badhub -d /home/badhub -m pi
  groupmod -n badhub pi
else
  useradd -m -s /bin/bash badhub
fi
for g in sudo video audio input render netdev plugdev gpio i2c spi users; do getent group $g >/dev/null && usermod -aG $g badhub; done
set +x; echo "badhub:${PI_PASSWORT}" | chpasswd; set -x
echo 'badhub ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/010_badhub-nopasswd
chmod 440 /etc/sudoers.d/010_badhub-nopasswd
rm -f /etc/sudoers.d/010_pi-nopasswd
systemctl disable userconfig.service 2>/dev/null || true
systemctl enable getty@tty1.service
# WLAN (NetworkManager) + Funk freigeben
set +x
cat > "/etc/NetworkManager/system-connections/${WLAN_SSID}.nmconnection" <<NM
[connection]
id=${WLAN_SSID}
type=wifi
autoconnect=true

[wifi]
mode=infrastructure
ssid=${WLAN_SSID}
hidden=true

[wifi-security]
key-mgmt=wpa-psk
psk=${WLAN_PSK}

[ipv4]
method=auto

[ipv6]
method=auto
NM
set -x
chmod 600 "/etc/NetworkManager/system-connections/${WLAN_SSID}.nmconnection"
raspi-config nonint do_wifi_country DE || true
cat > /etc/systemd/system/bts-wlan-freigabe.service <<'U'
[Unit]
Description=bts-light: WLAN-Funk freigeben (Image ohne Imager-Einrichtung)
Before=NetworkManager.service
[Service]
Type=oneshot
ExecStart=/usr/sbin/rfkill unblock wifi
[Install]
WantedBy=multi-user.target
U
systemctl enable bts-wlan-freigabe.service
raspi-config nonint do_change_timezone Europe/Berlin || true
raspi-config nonint do_configure_keyboard de || true
# Einrichtung wie am echten Pi, als badhub
cp /tmp/setup-monitor.sh /home/badhub/ && chown badhub: /home/badhub/setup-monitor.sh
# sudo ist unter der ARM-Emulation nicht setuid-fähig → als root mit einer
# sudo-Attrappe laufen, Umgebung wie bei badhub, danach Besitz korrigieren.
mkdir -p /tmp/shim && printf '#!/bin/sh\nexec "$@"\n' > /tmp/shim/sudo && chmod +x /tmp/shim/sudo
(cd /home/badhub && env HOME=/home/badhub USER=badhub LOGNAME=badhub SUDO_USER=badhub PATH=/tmp/shim:$PATH bash ./setup-monitor.sh </dev/null)
chown -R badhub: /home/badhub
rm -rf /tmp/shim
bash /tmp/build-master-image.sh
rm -f /tmp/setup-monitor.sh /tmp/build-master-image.sh /home/badhub/setup-monitor.sh
apt-get clean
IN

cat $R/boot/firmware/cmdline.txt
ls $R/boot/firmware/ | grep -i bts || true
rm -f $R/etc/resolv.conf; [ -e /tmp/resolv.orig ] && cp -a /tmp/resolv.orig $R/etc/resolv.conf || true
for d in dev/pts dev sys proc; do umount $R/$d; done
umount $R/boot/firmware; umount $R
zerofree ${LOOP}p2
losetup -d $LOOP
trap - EXIT
xz -T0 -6 -f "$IMG"
cd /build/out && sha256sum bts-light-pi.img.xz > bts-light-pi.img.xz.sha256 && ls -la
# Ergebnis dem aufrufenden Benutzer geben (Container läuft als root).
[ -n "${HOST_ID:-}" ] && chown -R "$HOST_ID" /build/out /build/dl
