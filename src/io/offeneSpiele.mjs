/** Die zwei Entscheidungen der Anzeige offener Paarungen (Spec
 *  `docs/features/tl-offene-paarungen.md`, ADR 0051/0052).
 *
 *  **Wo steht ein offenes Spiel?** Der Host liefert zwei getrennte Listen:
 *  die Arbeitsliste (`queue`) und die offenen Paarungen (`open_queue`), jede
 *  mit eigenem Deckel — sonst verdrängten Zukunftsspiele im 64-KiB-Fenster
 *  der Cloud genau die Spiele, um die es geht. Angezeigt werden sie trotzdem
 *  eingereiht: Jeder offene Eintrag trägt `queue_index`, „wie viele echte
 *  Wartespiele stehen vor mir". Hier werden beide Listen danach wieder
 *  zusammengeführt — ein Mischen zweier bereits sortierter Listen, **kein**
 *  zweites Sortieren. Die Reihenfolge bleibt serverseitig verbindlich, so wie
 *  die Spielliste es überall zusagt.
 *
 *  **Was steht in der Zeile?** Eine feststehende Seite zeigt ihre echten
 *  Namen, eine offene das Label vom Host („Müller oder Schmidt", „aus Spiel
 *  42", „noch offen"). Der Schrägstrich verbindet das Doppelpaar, das Wort
 *  „oder" die Kandidaten — deshalb entsteht der Doppel-Text hier und nicht
 *  aus einem zweiten Trennzeichen.
 *
 *  Kanonische Fassung. `tl.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build und können keine Module laden) — Änderungen hier
 *  und dort gemeinsam.
 */

/** Beschriftung, wenn der Host gar nichts sagt. */
export const OFFEN_TEXT = "noch offen";

/** Arbeitsliste und offene Paarungen zu einer Anzeige-Liste mischen.
 *
 *  Offene Einträge bekommen `offen: true`, damit die Zeile sie ohne zweiten
 *  Vergleich erkennt. Fehlt `open_queue` ganz — genau das liefert ein
 *  älterer Turnier-PC —, kommt die Arbeitsliste unverändert zurück.
 */
export function mischeOffene(queue, openQueue) {
  const echte = Array.isArray(queue) ? queue : [];
  const offene = Array.isArray(openQueue) ? openQueue : [];
  if (offene.length === 0) return echte;

  const gemischt = [];
  let n = 0; // wie viele echte Einträge schon ausgegeben sind
  for (const eintrag of offene) {
    const roh = Number(eintrag && eintrag.queue_index);
    const ziel = Math.max(0, Math.min(echte.length, Number.isFinite(roh) ? roh : 0));
    while (n < ziel) gemischt.push(echte[n++]);
    gemischt.push({ ...eintrag, offen: true });
  }
  while (n < echte.length) gemischt.push(echte[n++]);
  return gemischt;
}

/** Ist dieser Eintrag ein Spiel ohne feststehende Paarung? */
export function istOffen(eintrag) {
  return !!(eintrag && eintrag.offen);
}

/** Was in der Zeile auf der genannten Seite (1 oder 2) steht.
 *
 *  Feststehende Namen schlagen jedes Label: Ein halb offenes Spiel („Müller
 *  gegen den Sieger aus 42") zeigt links die echte Mannschaft und rechts die
 *  Herkunft.
 */
export function seitenText(eintrag, seite) {
  if (!eintrag) return OFFEN_TEXT;
  const namen = seite === 2 ? eintrag.team2 : eintrag.team1;
  if (Array.isArray(namen) && namen.length > 0) return namen.join(" / ");
  const label = seite === 2 ? eintrag.open_slot2_label : eintrag.open_slot1_label;
  return label && String(label).trim() ? String(label) : OFFEN_TEXT;
}
