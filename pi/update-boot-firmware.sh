#!/usr/bin/env bash
#
# bts-light Pi-Image — Start-Firmware auf der Boot-Partition erneuern
#
# Warum: Das gemeinsame Image (docs/pi-dual-image.md) ist ein Raspberry Pi OS
# vom 02.12.2020 mit Firmware vom 25.11.2020. Der Raspberry Pi 3 Model B+ in
# der aktuell ausgelieferten Platinen-Revision 1.4 (Revisionscode a020d4) hat
# einen anderen Spannungsregler-Baustein und startet erst mit Firmware ab
# September 2021. Mit der alten Firmware bleibt er vor dem Kernel hängen —
# kein Bild, kein Fehlertext.
#
# Was passiert hier: Nur die Start-Firmware (bootcode.bin, start*.elf,
# fixup*.dat) wird gegen den Buster-Stand 1.20220308_buster getauscht. Kernel,
# Gerätebeschreibungen (*.dtb), config.txt, cmdline.txt und das gesamte
# Root-Dateisystem bleiben unangetastet — der Eingriff ist damit so klein wie
# möglich und lässt sich über den Sicherungsordner zurücknehmen.
#
# Aufruf (die Boot-Partition ist FAT und an jedem Rechner einhängbar):
#   bash update-boot-firmware.sh /Volumes/boot        # SD-Karte am Mac
#   bash update-boot-firmware.sh /media/$USER/boot    # SD-Karte unter Linux
#   bash update-boot-firmware.sh --zurueck /Volumes/boot
#
# Für ein Image statt einer Karte: Boot-Partition des Images einhängen und
# denselben Aufruf darauf richten (siehe docs/pi-dual-image.md).

set -euo pipefail

# Fester Stand, damit jede Karte dieselbe Firmware bekommt. Die Prüfsummen
# unten gehören zu genau diesem Stand; wer ihn ändert, muss sie neu bilden.
FIRMWARE_TAG="1.20220308_buster"
FIRMWARE_URL="https://raw.githubusercontent.com/raspberrypi/firmware/${FIRMWARE_TAG}/boot"
SICHERUNG="firmware-alt"
# start.elf der Firmware vom 25.11.2020, wie sie im gemeinsamen Image steckt.
# Nur Karten mit genau diesem Stand werden angefasst — eine Karte mit neuerer
# Firmware (z. B. nach pi-setup.md eingerichtet) würde sonst zurückgestuft.
ALT_START="91fff95a02a19fbd5948dc37cc5f73e43abcc500532181bc900923ae4bcd7cdc"

# sha256  Dateiname — bewusst als Liste statt assoziativem Array, damit das
# Skript auch mit der alten bash 3.2 von macOS läuft.
PRUEFSUMMEN="\
69309823da13dc96b89e3d82b44f820e4f84efa79d207adad2c8784559794f03  bootcode.bin
071458b6f9443de1e9ec15edc776e990ecffd5bf6ea1e8e5c49fddf37aa532ee  fixup.dat
369e41a3149152f71d261e27a35994d3057a49aaec3225a17a17483e1108328d  fixup4.dat
727c017d61d7c4c464a6c2f0be868eed0744f55166a2e8c084475fb6fce935ec  fixup4cd.dat
c163d53fe80f12893d8d0b0f7c06a43cedd22ee11ea6651d3f2af8a215950ae8  fixup4db.dat
25dfb7b66310a3853f2bff1265aa26d5afaafbd09c6e04b97e94e817eeef13ab  fixup4x.dat
727c017d61d7c4c464a6c2f0be868eed0744f55166a2e8c084475fb6fce935ec  fixup_cd.dat
8dfca6f167158808e0e61a70d6ebe98c8bccc5bf97b52210c93ff8013085da80  fixup_db.dat
128cc1cbb9b2f20fefe93b833960c91eb9f41556df0fc38bfe11db0f7dffa9c4  fixup_x.dat
9b937f64f08ed53703ee343d4806256d4458a9a54e9c4fc4c65258f3fe388420  start.elf
fbaf930fc9685042d509de262303ef5605c3ae16f163848a82b5d305ea165311  start4.elf
de1c0fc4ad5cc37196c767c1fb0025f5eb26e87702bd7184f6910d9757f02c07  start4cd.elf
a8b80aa3f128b5a6e3b1ae7b94792d804ec4054a5b8294de3a1a13d16c4e81df  start4db.elf
d15167ad5fce4f0523c5ae514821edfda66e9ceb8a9132b6a7e1fbd00e11e288  start4x.elf
887478f87e8a8bcff7a01d19e1088f0fb96506972bbd4103b0e6faaf9c4d0a4a  start_cd.elf
c6ee2d8cfcae341fe111ed8a00ea79f142b5853bfb158a4e98d9e79d824aea31  start_db.elf
d7102fbce099b10bdc44cc006d7eb9e52876bfbad533015aae6436e3732eb1b9  start_x.elf"

abbruch() { echo "✗ $*" >&2; exit 1; }

# macOS bringt shasum mit, Linux sha256sum — beide liefern dasselbe Format.
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else shasum -a 256 "$1" | awk '{print $1}'; fi
}

ZURUECK=0
if [ "${1:-}" = "--zurueck" ]; then ZURUECK=1; shift; fi
BOOT="${1:-}"
[ -n "$BOOT" ] || abbruch "Aufruf: bash $0 [--zurueck] <Pfad zur Boot-Partition>"
BOOT="${BOOT%/}"

# Schutz vor dem falschen Ordner: Eine Pi-Boot-Partition erkennt man an
# diesen drei Dateien. Fehlt eine, wird nichts angefasst.
for f in bootcode.bin start.elf config.txt; do
  [ -f "$BOOT/$f" ] || abbruch "'$BOOT' ist keine Pi-Boot-Partition ($f fehlt)."
done
[ -w "$BOOT" ] || abbruch "'$BOOT' ist nicht beschreibbar (Karte schreibgeschützt?)."

# ─── Rückweg ────────────────────────────────────────────────────────────
if [ "$ZURUECK" = 1 ]; then
  [ -d "$BOOT/$SICHERUNG" ] || abbruch "Keine Sicherung unter '$BOOT/$SICHERUNG' gefunden."
  [ -f "$BOOT/$SICHERUNG/start.elf" ] || abbruch "Sicherung '$BOOT/$SICHERUNG' ist unvollständig."
  cp "$BOOT/$SICHERUNG"/* "$BOOT/"
  sync
  echo "✓ Alte Firmware aus '$SICHERUNG' zurückgespielt."
  exit 0
fi

# ─── 1) Schon erledigt? ─────────────────────────────────────────────────
# Alle Dateien vergleichen, nicht nur eine: Ein unterbrochener Lauf hinterlässt
# eine Mischung aus alt und neu, die sonst als „fertig" durchginge.
FERTIG=1
while read -r soll datei; do
  if [ ! -f "$BOOT/$datei" ] || [ "$(sha256 "$BOOT/$datei")" != "$soll" ]; then FERTIG=0; fi
done <<LISTE
$PRUEFSUMMEN
LISTE
if [ "$FERTIG" = 1 ]; then
  echo "✓ Firmware ist bereits auf Stand $FIRMWARE_TAG — nichts zu tun."
  exit 0
fi

# Nur das bekannte alte Image anfassen. Besteht die Sicherung schon, ist es
# die Fortsetzung eines unterbrochenen Laufs auf genau so einer Karte.
if [ ! -d "$BOOT/$SICHERUNG" ] && [ "$(sha256 "$BOOT/start.elf")" != "$ALT_START" ]; then
  abbruch "Diese Karte trägt nicht die Firmware vom 25.11.2020 des gemeinsamen Images — nichts geändert."
fi

# ─── 2) Laden und prüfen, BEVOR die Karte angefasst wird ────────────────
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "→ Lade Firmware $FIRMWARE_TAG …"
echo "$PRUEFSUMMEN" | while read -r soll datei; do
  curl -fsSL --retry 3 -o "$TMP/$datei" "$FIRMWARE_URL/$datei" \
    || abbruch "Download fehlgeschlagen: $datei"
  ist="$(sha256 "$TMP/$datei")"
  [ "$ist" = "$soll" ] || abbruch "Prüfsumme stimmt nicht: $datei (erwartet $soll, erhalten $ist)"
done
echo "  ✓ $(echo "$PRUEFSUMMEN" | wc -l | tr -d ' ') Dateien geladen, Prüfsummen stimmen"

# ─── 3) Alte Firmware sichern (nur beim ersten Lauf) ────────────────────
# Ein zweiter Lauf darf die Sicherung nicht mit der bereits neuen Firmware
# überschreiben — sonst gäbe es keinen Rückweg mehr.
if [ ! -d "$BOOT/$SICHERUNG" ]; then
  # Erst in einen Arbeitsordner, dann umbenennen: Bricht das Sichern ab, gilt
  # ein halber Ordner beim nächsten Lauf nicht als fertige Sicherung.
  rm -rf "$BOOT/$SICHERUNG.tmp"
  mkdir "$BOOT/$SICHERUNG.tmp"
  echo "$PRUEFSUMMEN" | while read -r _ datei; do
    if [ -f "$BOOT/$datei" ]; then cp "$BOOT/$datei" "$BOOT/$SICHERUNG.tmp/$datei"; fi
  done
  mv "$BOOT/$SICHERUNG.tmp" "$BOOT/$SICHERUNG"
  echo "→ Alte Firmware gesichert nach '$SICHERUNG/'"
else
  echo "→ Sicherung '$SICHERUNG/' besteht schon, bleibt unverändert"
fi

# ─── 4) Tauschen und gegenprüfen ────────────────────────────────────────
echo "$PRUEFSUMMEN" | while read -r soll datei; do
  cp "$TMP/$datei" "$BOOT/$datei" && [ "$(sha256 "$BOOT/$datei")" = "$soll" ] \
    || abbruch "Schreibfehler auf der Karte: $datei — Karte ist halb getauscht. Erneut ausführen oder mit --zurueck die Sicherung '$SICHERUNG/' zurückspielen."
done
sync

echo "✓ Firmware getauscht ($FIRMWARE_TAG). Karte sauber auswerfen, dann in den Pi."
echo "  Rückweg: bash \"$0\" --zurueck \"$BOOT\""
