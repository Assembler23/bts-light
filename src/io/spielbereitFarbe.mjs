// Zeilenfarbe der TL-Warteliste für eine spielbereite Partie (Meetingpoint).
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-spielbereit-farbe.mjs` prüft beide auf gleiches Verhalten.

/**
 * Wird diese Zeile grün statt gelb?
 *
 * Ein gerufenes Spiel trägt das Gelb der Aufrufmarke (`Match.Highlight` = 1,
 * ADR 0056). Sind am Meetingpoint alle eingecheckt, schlägt die Zeile auf
 * Grün um — „gerufen" ist dann erledigt, „kann aufs Feld" ist die Aussage.
 *
 * Eine von Hand gesetzte andere Farbe (2–6) bleibt stehen: Sie bedeutet der
 * Turnierleitung etwas Eigenes, und das Abzeichen „✓ spielbereit" steht
 * ohnehin an der Zeile. Ohne Farbe (0) wird die Zeile ebenfalls grün — das
 * Gelb kann fehlen, solange BTP den Aufruf noch nicht zurückgemeldet hat.
 *
 * Reine Anzeige: Nach BTP wird dafür nichts geschrieben, dort bleibt Gelb.
 */
export function spielbereitGruen(m) {
  if (!m || !m.prep_call || !m.meetingpoint || !m.meetingpoint.bereit) return false;
  return (m.highlight || 0) <= 1;
}
