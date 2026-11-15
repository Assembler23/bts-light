/**
 * TP-Datei im Setup: Entscheidung über eine gewählte oder in BTP gefundene
 * Datei (Spec tp-datei-finden, AK1–AK5; ADR 0097) — Modul `src/io/tpWahl.mjs`.
 */
import assert from "node:assert/strict";
import { tpInfoText, tpSucheText, tpWahl } from "../src/io/tpWahl.mjs";

const A = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA";
const B = "9AA7FE89-0000-4000-8000-000000000001";
const erkannt = (guid) => ({ art: "erkannt", groesse: 1, anzeige: "x", guid });
const wahl = (teil) =>
  tpWahl({ name: "t.TP", eingetragen: "", feldLeer: true, quelle: "suche", ...teil });

// AK1: gleiche Kennung → still übernehmen, Kennung oben bleibt.
assert.deepEqual(wahl({ info: erkannt(A), eingetragen: A, feldLeer: false }), {
  art: "uebernehmen",
  bindung: A,
  kennungNeu: null,
});

// AK2: Feld leer → Datei und Kennung still übernehmen.
assert.deepEqual(wahl({ info: erkannt(A) }), {
  art: "uebernehmen",
  bindung: A,
  kennungNeu: A,
});

// AK3: andere Kennung → Rückfrage, für Dialog wie Suche.
for (const quelle of ["dialog", "suche"]) {
  assert.deepEqual(wahl({ info: erkannt(B), eingetragen: A, feldLeer: false, quelle }), {
    art: "frage",
    dateiGuid: B,
    eingetragen: A,
  });
}

// Im Feld steht etwas, das keine Kennung ist → fragen statt ersetzen.
assert.deepEqual(wahl({ info: erkannt(A), feldLeer: false }), {
  art: "frage",
  dateiGuid: A,
  eingetragen: "",
});

// AK4: Datei ohne Kennung. Von der Suche gefunden → Rückfrage ohne
// Kennungs-Wechsel; von Hand gewählt → wie bisher still gebunden.
assert.deepEqual(wahl({ info: erkannt(null), eingetragen: A, feldLeer: false }), {
  art: "frage",
  dateiGuid: "",
  eingetragen: A,
});
assert.deepEqual(
  wahl({ info: erkannt(null), eingetragen: A, feldLeer: false, quelle: "dialog" }),
  { art: "uebernehmen", bindung: A, kennungNeu: null },
);

// Ohne Kennung in Datei UND Feld → Fehler, nichts wird eingetragen.
const ohne = wahl({ info: erkannt(null) });
assert.equal(ohne.art, "fehler");
assert.match(ohne.text, /nennt keine eindeutige Turnier-Kennung/);

// Datei fällt durch die Prüfung → Fehler mit Dateiname.
for (const info of [{ art: "keine_tp_datei" }, { art: "zu_gross", groesse: 9 }, { art: "nicht_gefunden" }]) {
  const w = wahl({ info, eingetragen: A, feldLeer: false });
  assert.equal(w.art, "fehler");
  assert.ok(w.text.startsWith("„t.TP“: "), w.text);
  assert.ok(w.text.endsWith(tpInfoText(info)));
}
assert.equal(tpInfoText(erkannt(A)), null);

// AK5: jede Lage der Suche hat ihre Meldung …
const lagen = {
  btp_nicht_gestartet: /BTP läuft nicht auf diesem PC/,
  kein_turnier_offen: /kein Turnier geöffnet/,
  kein_zugriff: /als Administrator/,
  haenger: /nicht geantwortet/,
  mehrere: /Mehrere Turnierdateien/,
  nicht_lokal: /auf einem anderen PC/,
};
for (const [art, muster] of Object.entries(lagen)) {
  assert.match(tpSucheText({ art }), muster, art);
}
assert.equal(
  tpSucheText({ art: "vorpruefung", name: "alt.TP", info: { art: "zu_gross", groesse: 9 } }),
  "„alt.TP“: Die Datei ist größer als 32 MB — zu groß für den Upload.",
);
assert.equal(
  tpSucheText({ art: "netzlaufwerk", name: "netz.TP" }),
  "„netz.TP“ liegt auf einem Netzlaufwerk — bitte die Datei von Hand wählen.",
);
// … und wo es nichts zu sagen gibt, bleibt es still.
for (const art of ["gefunden", "laeuft_schon", "nicht_verfuegbar"]) {
  assert.equal(tpSucheText({ art }), null, art);
}

console.log("test-tp-wahl: OK");
