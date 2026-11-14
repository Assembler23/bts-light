// Schriftgröße der Ansage im Schiri-Modus des Zähl-Tablets.
//
// Schiedsrichter mit schwächerem Sehvermögen wünschten sich größere Ansage-
// texte (Rückmeldung 14.11.2026). Die Ansage stand bis dahin fest auf
// 1,6 rem — egal, ob „7 beide" oder die ganze Eröffnung dastand, und egal,
// wie viel Platz das Tablet hatte. Jetzt nimmt sie die GRÖSSTE Stufe, die
// passt: Kurze Ansagen werden deutlich größer, lange nie kleiner als bisher
// — außer sie liefen sonst aus dem Bild (in der schmalen Seitenspalte der
// Anordnung „übereinander" tat die Eröffnung genau das).
//
// Ob eine Größe passt, kann nur der Browser messen (Umbruch, Doppelnamen).
// Die Entscheidung selbst steht hier, damit sie ohne Browser prüfbar ist.
//
// `src-tauri/assets/tablet.html` trägt eine Inline-Kopie (kein Modul-Import
// möglich); `scripts/test-inline-kopien.mjs` vergleicht ihr Verhalten mit
// diesem Modul. Deshalb: schlichte `function`-Deklarationen.

/** Stufen in rem: `max` für kurze Ansagen, `basis` = darunter geht es nur,
 *  wenn die Ansage sonst überliefe, `not` = kleinste noch lesbare Größe
 *  (darunter scrollt die Leiste). `feld` = Anteil der Mittelspalte, der dem
 *  Spielfeld bleiben soll, wenn es UNTER der Leiste steht. */
function ansageStufen(gross) {
  return gross
    ? { max: 3.6, basis: 2.0, not: 1.2, feld: 0.25 }
    : { max: 2.6, basis: 1.6, not: 1.0, feld: 0.38 };
}

/** Kandidaten von groß nach klein, in Schritten von 0,2 rem. */
function ansageKandidaten(gross) {
  const s = ansageStufen(gross);
  const liste = [];
  for (let z = Math.round(s.max * 10); z >= Math.round(s.not * 10); z -= 2) liste.push(z / 10);
  return liste;
}

/** Wählt die Schriftgröße (rem). `messe(rem)` setzt die Größe probeweise und
 *  liefert `{ ueberlauf, feldAnteil }`: läuft die Leiste aus ihrem Bereich,
 *  und welchen Anteil behält das Spielfeld (`null`, wenn es NEBEN der Leiste
 *  steht und von ihr nichts abgeben muss).
 *  - Über `basis`: nur, wenn nichts überläuft UND das Feld genug behält.
 *  - Bei `basis`: reicht, dass nichts überläuft (so war es schon immer).
 *  - Unter `basis`: nur, um einen Überlauf zu heilen; passt gar nichts,
 *    bleibt die kleinste Stufe (die Leiste scrollt dann). */
function ansageSchriftWaehlen(gross, messe) {
  const s = ansageStufen(gross);
  const kandidaten = ansageKandidaten(gross);
  for (const rem of kandidaten) {
    const m = messe(rem) || {};
    if (m.ueberlauf) continue;
    if (rem <= s.basis) return rem;
    if (m.feldAnteil == null || m.feldAnteil >= s.feld) return rem;
  }
  return kandidaten[kandidaten.length - 1];
}

/** Gespeicherter Schalter „Große Schrift" (`'1'` = an). */
function grosseSchriftAus(roh) {
  return roh === '1';
}

export { ansageStufen, ansageKandidaten, ansageSchriftWaehlen, grosseSchriftAus };
