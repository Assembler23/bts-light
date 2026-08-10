// Gruppiert die Spielerliste einer Check-In-Klasse zu Anzeige-Zeilen:
// Zwei Spieler derselben Meldung (gleiche `entry_id`) sind ein Doppel und
// stehen in EINER Zeile; alle anderen bleiben einzeln.
//
// Ein Paar entsteht NUR, wenn `entry_id > 0` ist UND genau zwei Spieler
// sie teilen. Beides ist Schutz, keine Kür: Ein badhub vor der
// entry_id-Auslieferung schickt überall 0 — ohne die Schranke klumpte die
// ganze Klasse zu einer Zeile zusammen. Und bei 3+ Trägern derselben
// Meldung (Datenfehler in BTP) sind drei ehrliche Einzelzeilen besser als
// ein erfundenes „Doppel zu dritt". Unvollständige Doppel (ein Träger)
// bleiben ebenfalls einzeln — deckungsgleich mit dem Verhalten des Pushs,
// der sie als Einzel-Meldung weitergibt.
//
// Reihenfolge: Ein Paar steht an der Position seines ersten Partners; der
// zweite wird zu ihm gezogen. Alles andere behält die Server-Reihenfolge.
export function pairEntries(players) {
  const list = Array.isArray(players) ? players : [];
  const traeger = new Map();
  for (const p of list) {
    const id = Number(p?.entry_id) || 0;
    if (id > 0) traeger.set(id, (traeger.get(id) || 0) + 1);
  }
  const abgeraeumt = new Set();
  const out = [];
  for (const p of list) {
    const id = Number(p?.entry_id) || 0;
    if (id > 0 && traeger.get(id) === 2) {
      if (abgeraeumt.has(id)) continue;
      abgeraeumt.add(id);
      out.push(list.filter((q) => (Number(q?.entry_id) || 0) === id));
    } else {
      out.push([p]);
    }
  }
  return out;
}
