// Was das Zähl-Tablet tut, wenn seine Seite wieder sichtbar wird (Hülle
// aufgeklappt, Bildschirm an, App wieder vorn).
//
// Warum: Solange das Tablet schläft, stehen seine Timer still. Der
// Verbindungs-Wächter (alle 5 s) merkt eine tote Verbindung dann erst
// ein bis zwei Takte nach dem Aufwachen (bis ~10 s, plus erneute Anmeldung).
// Beim Aufwachen prüfen wir sofort, statt auf den nächsten Takt zu warten.
// Anlass: Logcheck Berlin-Senior-Cup 31.10.2026 (Tablets zwischen den
// Spielen zugeklappt, Ursache im Log nicht erkennbar).
//
// Inline-Kopie in `src-tauri/assets/tablet.html` zwischen den Marken
// `<tabletAufwachen.mjs>` (als Block-Kommentar) — `scripts/test-tablet-aufwachen.mjs`
// prüft, dass beide wortgleich sind.

/* <tabletAufwachen.mjs> */
// Ab dieser Stille gilt eine offene Verbindung als tot (wie der Wächter).
var AUFWACH_STILLE_MS = 15000;
// So lange wartet die Probe nach dem Aufwachen auf ein Lebenszeichen.
var AUFWACH_PROBE_MS = 3000;

// zustand: WebSocket.readyState (0 verbindet, 1 offen, 2 schließt, 3 zu)
// oder null, wenn es keinen Socket gibt.
// Ergebnis: 'verbinden' (keiner/zu → jetzt neu), 'ersetzen' (offen, aber
// zu lange still → schließen und sofort neu), 'pruefen' (offen und frisch
// → Ping schicken und nach AUFWACH_PROBE_MS nachsehen), 'warten'
// (verbindet oder schließt gerade → nichts anfassen, sonst doppelte Sockets).
function aufwachAktion(zustand, letzterKontaktMs, jetztMs) {
  if (zustand === null || zustand === undefined || zustand === 3) return 'verbinden';
  if (zustand !== 1) return 'warten';
  if (!(letzterKontaktMs > 0) || jetztMs - letzterKontaktMs > AUFWACH_STILLE_MS) return 'ersetzen';
  return 'pruefen';
}

// Nach der Probe: Kam seit dem Ping nichts vom Server, ist die Verbindung tot.
function probeGescheitert(letzterKontaktMs, pingMs) {
  return !(letzterKontaktMs >= pingMs);
}
/* </tabletAufwachen.mjs> */

export { AUFWACH_STILLE_MS, AUFWACH_PROBE_MS, aufwachAktion, probeGescheitert };
