/**
 * Update-Banner, Seiten-Teil: jede Zeile der Tabelle „Banner und
 * Wartungsseite" aus docs/features/hotfix-ohne-neustart.md — Modul
 * `src/io/updateBanner.mjs`.
 *
 * Warum: Der Text entscheidet, ob die Turnierleitung mitten im Turnier
 * klickt. Eine Lage ohne Zeile oder mit falschem Knopf fiele erst dort auf.
 */
import assert from "node:assert/strict";
import {
  KNOPF,
  kernZeile,
  seitenZeile,
  unterbrechungsHinweis,
  wartungSeiten,
  wechselHinweis,
} from "../src/io/updateBanner.mjs";

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

// ── Kern-Zeile: Update der App selbst ──────────────────────────────────────

const kern = (teil = {}) => ({
  tausch: "aus",
  version: null,
  rueckweg: false,
  grund: null,
  meldung: "keine",
  meldung_version: null,
  meldung_grund: null,
  ...teil,
});
const app = (teil = {}, kernTeil = {}) => ({
  phase: "ready",
  version: "0.9.440",
  notes: null,
  message: null,
  occupied_courts: 3,
  sync_running: true,
  install_on_exit: false,
  app_version: "0.9.439",
  seiten: seiten(),
  kern: kern(kernTeil),
  ...teil,
});

// Nichts zu tun: kein Banner.
for (const phase of ["idle", "checking", "current", "error"]) {
  assert.equal(kernZeile(app({ phase })), null, phase);
}
assert.equal(kernZeile(null), null);

// Kern bereit, Tausch möglich: „Jetzt wechseln", wenige Sekunden.
let k = kernZeile(app({}, { tausch: "bereit", version: "0.9.440", rueckweg: true }));
assert.equal(k.text, "Update (v0.9.440) liegt bereit.");
assert.deepEqual(k.zusatz, [
  "Auf 3 Feldern laufen Spiele – der Wechsel unterbricht wenige Sekunden, die Übertragung läuft von selbst weiter.",
]);
assert.deepEqual(k.knoepfe, ["beim_beenden", "wechseln"]);
assert.equal(KNOPF.wechseln, "Jetzt wechseln");
assert.equal(KNOPF.beim_beenden, "Beim Beenden einbauen");
assert.match(
  wechselHinweis(app({ occupied_courts: 1 })),
  /^Auf 1 Feld läuft ein Spiel – der Wechsel unterbricht wenige Sekunden/,
);
assert.match(wechselHinweis(app({ occupied_courts: 0 })), /^Kein Spiel läuft – der Wechsel/);
assert.equal(wechselHinweis(app({ sync_running: false })), "Der Wechsel dauert wenige Sekunden.");

// Dito, die neue Version erlaubt keinen Rückweg: Das steht VOR dem Klick da.
k = kernZeile(app({}, { tausch: "bereit", version: "0.9.440", rueckweg: false }));
assert.equal(k.zusatz.length, 2);
assert.equal(k.zusatz[1], "Ein Zurück zu v0.9.439 ist nach diesem Update nicht möglich.");
assert.deepEqual(k.knoepfe, ["beim_beenden", "wechseln"]);

// Kern bereit, Tausch nicht möglich: der bisherige Installer-Weg, 20 s.
for (const tausch of ["aus", "laedt", "nicht_moeglich"]) {
  k = kernZeile(app({}, { tausch, grund: "nur unter Windows" }));
  assert.deepEqual(k.knoepfe, ["beim_beenden", "neu_starten"], tausch);
  assert.match(k.zusatz[0], /der Neustart unterbricht etwa 20 s/, tausch);
  assert.doesNotMatch(JSON.stringify(k), /nur unter Windows/, "der Grund ist Diagnose, kein Bannertext");
}
assert.equal(KNOPF.neu_starten, "Jetzt neu starten");
assert.equal(unterbrechungsHinweis(app({ sync_running: false })), "Die App startet dafür kurz neu.");
// Die vorgeladene Datei gehört zu einer ANDEREN Version als das Update: kein Wechsel.
k = kernZeile(app({}, { tausch: "bereit", version: "0.9.441", rueckweg: true }));
assert.deepEqual(k.knoepfe, ["beim_beenden", "neu_starten"]);

// Beim Beenden vorgemerkt: kein „Jetzt"-Knopf, nur die Rücknahme.
k = kernZeile(app({ install_on_exit: true }, { tausch: "bereit", version: "0.9.440" }));
assert.deepEqual(k.zusatz, ["Es wird beim Beenden von bts-light still eingebaut."]);
assert.deepEqual(k.knoepfe, ["doch_nicht"]);

// Laden und Einbau/Wechsel: nur Text.
k = kernZeile(app({ phase: "downloading" }));
assert.equal(k.text, "Update (v0.9.440) wird im Hintergrund geladen …");
assert.deepEqual(k.knoepfe, []);
k = kernZeile(app({ phase: "installing" }));
assert.match(k.text, /wird eingebaut – die App startet gleich neu/);
k = kernZeile(app({}, { tausch: "wechselt", version: "0.9.440" }));
assert.equal(k.text, "Wechsel auf v0.9.440 läuft – bts-light ist gleich wieder da.");
assert.deepEqual(k.knoepfe, []);

// Nach dem Wechsel: Die neue Version sagt, dass sie läuft.
k = kernZeile(app({ phase: "current", version: null, app_version: "0.9.440" }, { meldung: "gewechselt", meldung_version: "0.9.439" }));
assert.equal(k.text, "bts-light läuft jetzt mit v0.9.440.");
assert.deepEqual(k.knoepfe, ["schliessen"]);

// Start-Wächter hat zurückgesetzt: Vorrang vor allem, auch vor dem Angebot.
k = kernZeile(
  app({}, { tausch: "nicht_moeglich", meldung: "zurueckgefallen", meldung_version: "0.9.440", meldung_grund: "Frist" }),
);
assert.equal(k.art, "zurueckgefallen");
assert.equal(
  k.text,
  "v0.9.440 ließ sich nicht starten. bts-light läuft wieder mit v0.9.439, die Übertragung läuft.",
);
assert.deepEqual(k.knoepfe, ["verstanden"]);
assert.equal(KNOPF.verstanden, "Verstanden");
// Läuft die Übertragung nicht, behauptet der Text es auch nicht.
k = kernZeile(app({ sync_running: false }, { meldung: "zurueckgefallen", meldung_version: "0.9.440" }));
assert.equal(k.text, "v0.9.440 ließ sich nicht starten. bts-light läuft wieder mit v0.9.439.");

console.log("OK — Update-Banner: alle Lagen der Seiten- und der Kern-Zeile geprüft.");
