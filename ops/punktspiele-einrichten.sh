#!/usr/bin/env bash
# ops/punktspiele-einrichten.sh — Server für Punktspiele ohne Windows-PC einrichten (einmalig, als root).
#
# Richtet ein (docs/features/punktspiel-kopflos.md, ADR 0108, badhub ADR-0092 Schritt 4):
#   - Laufzeit-Bibliothek von WebKitGTK (bts-light ist dieselbe Programmdatei wie am Desktop und hängt an ihr,
#     auch wenn ohne Oberfläche nie ein Fenster entsteht)
#   - Benutzer bts-punktspiele (ohne Anmeldung), dem auch badhub (php-fpm) als Gruppenmitglied angehört
#   - Spool je System: /var/www/badhub_data/ls-hosts (Prod) und ls-hosts-test (Test), badhub:bts-punktspiele 2770
#   - /opt/bts-light-host für die Programmdatei (Auslieferung durch bts-deploy, wie beim Relay)
#   - systemd-Unit bts-punktspiele@prod und @test, sudoers-Regel für den Neustart durch bts-deploy
#
# Idempotent. Startet die Dienste erst, wenn die Programmdatei da ist (Workflow „Punktspiel-Host deployen“).
#
# Danach von Hand, in BEIDEN .env von badhub (geteilter php-fpm-Pool):
#   /var/www/badhub/.env        LS_HOST_SPOOL_DIR=/var/www/badhub_data/ls-hosts
#   /var/www/badhub-test/.env   LS_HOST_SPOOL_DIR=/var/www/badhub_data/ls-hosts-test
# und: sudo systemctl reload php8.3-fpm
#
# Aufruf: sudo bash ops/punktspiele-einrichten.sh
set -euo pipefail

[ "$(id -u)" -eq 0 ] || { echo "Bitte mit sudo starten."; exit 1; }
HIER="$(cd "$(dirname "$0")" && pwd)"

echo "1/6 Laufzeit-Bibliothek"
dpkg -s libwebkit2gtk-4.1-0 >/dev/null 2>&1 || { apt-get update -q; apt-get install -y --no-install-recommends libwebkit2gtk-4.1-0; }

echo "2/6 Benutzer und Gruppe"
id bts-punktspiele >/dev/null 2>&1 || useradd --system --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin bts-punktspiele
# badhub (php-fpm) legt Aufträge ab und schreibt die Marke „ende“: Mitglied der Gruppe des Dienstes
id -nG badhub | tr ' ' '\n' | grep -qx bts-punktspiele || usermod -aG bts-punktspiele badhub

echo "3/6 Spool"
for spool in /var/www/badhub_data/ls-hosts /var/www/badhub_data/ls-hosts-test; do
    install -d -o badhub -g bts-punktspiele -m 2770 "$spool"
done

echo "4/6 Programmordner"
install -d -o badhub -g bts-deploy -m 2775 /opt/bts-light-host

echo "5/6 Dienst"
install -d -m 0755 /etc/bts-punktspiele
[ -f /etc/bts-punktspiele/prod.env ] || echo "SPOOL=/var/www/badhub_data/ls-hosts" > /etc/bts-punktspiele/prod.env
[ -f /etc/bts-punktspiele/test.env ] || echo "SPOOL=/var/www/badhub_data/ls-hosts-test" > /etc/bts-punktspiele/test.env
install -m 0644 "$HIER/bts-punktspiele@.service" /etc/systemd/system/bts-punktspiele@.service
systemctl daemon-reload
systemctl enable bts-punktspiele@prod bts-punktspiele@test >/dev/null

echo "6/6 Neustart durch den Deploy"
REGEL=/etc/sudoers.d/bts-punktspiele
cat > "$REGEL.neu" <<'SUDO'
bts-deploy ALL=(root) NOPASSWD: /usr/bin/systemctl restart bts-punktspiele@prod, /usr/bin/systemctl restart bts-punktspiele@test
SUDO
chmod 0440 "$REGEL.neu"
visudo -cf "$REGEL.neu" >/dev/null && mv "$REGEL.neu" "$REGEL" || { rm -f "$REGEL.neu"; echo "   ABBRUCH: sudoers-Regel ungültig."; exit 1; }

if [ -x /opt/bts-light-host/bts-light ]; then
    systemctl restart bts-punktspiele@prod bts-punktspiele@test
    echo "Fertig. Dienste laufen: $(systemctl is-active bts-punktspiele@prod) / $(systemctl is-active bts-punktspiele@test)"
else
    echo "Fertig. Die Programmdatei fehlt noch: Workflow „Punktspiel-Host deployen“ in GitHub starten, er startet die Dienste."
fi
echo "Nicht vergessen: LS_HOST_SPOOL_DIR in beiden .env von badhub setzen und php8.3-fpm neu laden (siehe Kopf dieser Datei)."
echo "Neue Gruppenzugehörigkeit von badhub gilt für php-fpm erst nach: sudo systemctl restart php8.3-fpm"
