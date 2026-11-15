/**
 * Behandlungsuhr am Tablet (Spec verletzungsunterbrechung, ADR 0095).
 * Prüft die kanonische Fassung aus `src/io/behandlungsUhr.mjs`; die
 * Inline-Kopie in `tablet.html` prüft `scripts/test-inline-kopien.mjs`.
 */
import assert from "node:assert/strict";
import {
  RICHTZEIT_STANDARD_S,
  hostKenntBehandlung,
  richtzeitAusBrief,
  uhrText,
  behandlungsUhr,
} from "../src/io/behandlungsUhr.mjs";

// Uhrtext
assert.equal(uhrText(0), "0:00");
assert.equal(uhrText(59), "0:59");
assert.equal(uhrText(60), "1:00");
assert.equal(uhrText(4499), "74:59");
assert.equal(uhrText(4503), "75:03");
assert.equal(uhrText(-5), "0:00");
assert.equal(uhrText(NaN), "0:00");

// Uhr: Schwelle genau bei der Richtzeit
const T = 1_755_600_000_000;
assert.equal(behandlungsUhr(T + 299_999, T, 300).ueberschritten, false);
assert.equal(behandlungsUhr(T + 299_999, T, 300).text, "4:59");
assert.equal(behandlungsUhr(T + 300_000, T, 300).ueberschritten, true);
assert.equal(behandlungsUhr(T + 300_000, T, 300).text, "5:00");
assert.equal(behandlungsUhr(T + 4_503_000, T, 300).text, "75:03");
// Startzeit in der Zukunft: 0:00, nicht rückwärts
assert.deepEqual(behandlungsUhr(T - 5000, T, 300), { sekunden: 0, text: "0:00", ueberschritten: false });
// Ohne Startzeit keine Uhr
for (const s of [undefined, null, 0, NaN, "123", -1]) {
  assert.equal(behandlungsUhr(T, s, 300), null, String(s));
}

// Richtzeit aus dem Brief
assert.equal(RICHTZEIT_STANDARD_S, 300);
assert.equal(richtzeitAusBrief(null), 300);
assert.equal(richtzeitAusBrief({}), 300);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: 0 }), 300);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: -60 }), 300);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: NaN }), 300);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: "600" }), 300);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: 1e9 }), 1800);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: 10 }), 60);
assert.equal(richtzeitAusBrief({ injuryGuideSecs: 420 }), 420);

// Fähigkeitsmeldung
assert.equal(hostKenntBehandlung(null), false);
assert.equal(hostKenntBehandlung({}), false);
assert.equal(hostKenntBehandlung({ injuryGuideSecs: 0 }), false);
assert.equal(hostKenntBehandlung({ injuryGuideSecs: "300" }), false);
assert.equal(hostKenntBehandlung({ injuryGuideSecs: Infinity }), false);
assert.equal(hostKenntBehandlung({ injuryGuideSecs: 300 }), true);

console.log("test-behandlungs-uhr: alles grün");
