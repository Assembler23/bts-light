/**
 * Update-Banner, Seiten-Teil: jede Zeile der Tabelle „Banner und
 * Wartungsseite" aus docs/features/hotfix-ohne-neustart.md — Modul
 * `src/io/updateBanner.mjs`.
 *
 * Warum: Der Text entscheidet, ob die Turnierleitung mitten im Turnier
 * klickt. Eine Lage ohne Zeile oder mit falschem Knopf fiele erst dort auf.
 */
import assert from "node:assert/strict";
import { KNOPF, seitenZeile, wartungSeiten } from "../src/io/updateBanner.mjs";

const seiten = (teil = {}) => ({
  aktive_folge: 0,
  bereit_folge: null,
  meldung: "keine",
  meldung_folge: null,
  reichweite: "lan",
  verbund: false,
  abgelehnt: null,
  rueckweg: false,
  ...teil,
});
const info = (teil = {}) => ({ app_version: "0.9.433", seiten: seiten(teil) });

// Nichts bereit, nichts passiert: kein Banner.
assert.equal(seitenZeile(seiten()), null);
assert.equal(seitenZeile(null), null);
assert.equal(seitenZeile(undefined), null);

// Seiten-Paket passt, LAN.
let z = seitenZeile(seiten({ bereit_folge: 2 }));
assert.equal(z.art, "angebot");
assert.equal(
  z.text,
  "Verbesserung für Tablets und Anzeigen liegt bereit (Seiten-Stand 2) – ohne Unterbrechung.",
);
assert.deepEqual(z.zusatz, []);
assert.deepEqual(z.knoepfe, ["uebernehmen"]);
assert.equal(KNOPF.uebernehmen, "Jetzt übernehmen");

// Dito, LAN + Cloud: der Reichweiten-Satz.
z = seitenZeile(seiten({ bereit_folge: 2, reichweite: "lan+cloud" }));
assert.deepEqual(z.zusatz, [
  "Gilt für Geräte im Hallennetz. Geräte über die Cloud bekommen ihre Seiten vom Server.",
]);
assert.deepEqual(z.knoepfe, ["uebernehmen"]);

// Nur Cloud: kein Seiten-Angebot — auch wenn der Kern eines meldete.
assert.equal(seitenZeile(seiten({ bereit_folge: 2, reichweite: "keine" })), null);
assert.deepEqual(wartungSeiten(info({ reichweite: "keine" })).hinweise, [
  "Im Cloud-Betrieb kommen die Seiten vom Server.",
]);
assert.deepEqual(wartungSeiten(info({ reichweite: "keine", bereit_folge: 2 })).hinweise, [
  "Im Cloud-Betrieb kommen die Seiten vom Server.",
]);

// Slave-Verbund: Zusatz, auch zusammen mit LAN + Cloud.
z = seitenZeile(seiten({ bereit_folge: 1, verbund: true }));
assert.deepEqual(z.zusatz, ["Weitere PCs (andere Halle) werden dort einzeln aktualisiert."]);
assert.equal(seitenZeile(seiten({ bereit_folge: 1, verbund: true, reichweite: "lan+cloud" })).zusatz.length, 2);

// Seiten übernommen: sagt, was mit welchem Gerät passiert, und bietet den Rückweg.
z = seitenZeile(seiten({ meldung: "uebernommen", meldung_folge: 2, aktive_folge: 2, rueckweg: true }));
assert.equal(z.art, "uebernommen");
assert.match(z.text, /^Seiten aktualisiert\./);
assert.match(z.text, /Tablets laden neu, sobald kein Spiel läuft/);
assert.match(z.text, /Monitore über „Neu laden“/);
assert.match(z.text, /Turnierleitungs-Seite bitte von Hand neu laden/);
assert.deepEqual(z.knoepfe, ["zurueck", "schliessen"]);
assert.equal(KNOPF.zurueck, "Zurück zum vorigen Stand");
assert.equal(KNOPF.schliessen, "Schließen");
// Die Meldung hat Vorrang vor einem neuen Angebot.
assert.equal(
  seitenZeile(seiten({ meldung: "uebernommen", bereit_folge: 3, rueckweg: true })).art,
  "uebernommen",
);
// Der Reichweiten-Satz gilt auch nach der Übernahme.
assert.equal(
  seitenZeile(seiten({ meldung: "uebernommen", reichweite: "lan+cloud", rueckweg: true })).zusatz.length,
  1,
);

// Nach Rückweg: nennt den Stand und dass er nicht wiederkommt.
z = seitenZeile(seiten({ meldung: "zurueckgenommen", meldung_folge: 2, aktive_folge: 1, rueckweg: true }));
assert.equal(z.art, "zurueckgenommen");
assert.equal(z.text, "Seiten-Stand 2 wurde zurückgenommen und wird nicht erneut angeboten.");
assert.deepEqual(z.knoepfe, ["schliessen"]);
assert.match(seitenZeile(seiten({ meldung: "zurueckgenommen" })).text, /^Der Seiten-Stand wurde/);

// Paket abgelehnt: kein Banner, nur ein Grund auf der Wartungsseite.
assert.equal(seitenZeile(seiten({ abgelehnt: "Signatur: Signatur passt nicht" })), null);
assert.deepEqual(wartungSeiten(info({ abgelehnt: "Signatur: Signatur passt nicht" })).hinweise, [
  "Paket abgelehnt, nichts geändert (Signatur: Signatur passt nicht).",
]);

// Wartungsseite: dauerhaft „App vX · Seiten-Stand N", N = 0 heißt eingebaut.
let w = wartungSeiten(info());
assert.equal(w.stand, "App v0.9.433 · Seiten-Stand 0 (eingebaut)");
assert.equal(w.rueckweg, false);
assert.deepEqual(w.hinweise, []);
w = wartungSeiten(info({ aktive_folge: 3, rueckweg: true, bereit_folge: 4 }));
assert.equal(w.stand, "App v0.9.433 · Seiten-Stand 3");
assert.equal(w.rueckweg, true);
assert.deepEqual(w.hinweise, ["Seiten-Stand 4 liegt bereit – Übernahme über das Banner oben."]);
assert.equal(wartungSeiten(null), null);
assert.equal(wartungSeiten({ app_version: "1" }), null);

console.log("OK — Update-Banner (Seiten): alle Lagen geprüft.");
