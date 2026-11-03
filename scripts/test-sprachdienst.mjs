// Tests für src/io/sprachdienst.mjs + Drift gegen Rust `config::dienst_art`.
// Aufruf: node scripts/test-sprachdienst.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  DIENSTE,
  cloudStimmeVollstaendig,
  dienstArt,
  dienstName,
  dienstNormalisiert,
} from "../src/io/sprachdienst.mjs";

let n = 0;
function pruefe(name, f) {
  f();
  n++;
  console.log("✓", name);
}

pruefe("unbekannt und leer gelten als Azure", () => {
  assert.equal(dienstArt(undefined), "azure");
  assert.equal(dienstArt(""), "azure");
  assert.equal(dienstArt("elevenlabs_v9"), "azure");
  assert.equal(dienstName("quatsch"), "Azure");
});

pruefe("Anbieter je Wert", () => {
  assert.equal(dienstArt("elevenlabs_flash"), "elevenlabs");
  assert.equal(dienstArt("openai"), "openai");
  assert.equal(dienstArt("gemini_38_lite"), "gemini");
});

pruefe("vollständig nur mit dem Schlüssel des gewählten Dienstes", () => {
  const az = { enabled: true, key: "a", region: "we", dienst: "azure" };
  assert.equal(cloudStimmeVollstaendig(az), true);
  assert.equal(cloudStimmeVollstaendig({ ...az, dienst: "openai" }), false);
  assert.equal(cloudStimmeVollstaendig({ ...az, dienst: "openai", openai_key: "x" }), true);
  assert.equal(cloudStimmeVollstaendig({ ...az, dienst: "gemini_38_flash", gemini_key: " " }), false);
  assert.equal(cloudStimmeVollstaendig({ ...az, enabled: false }), false);
});

pruefe("alte Gemini-Werte landen wie im Kern bei 3.8 Flash", () => {
  assert.equal(dienstNormalisiert("gemini_flash"), "gemini_38_flash");
  assert.equal(dienstNormalisiert("gemini_pro"), "gemini_38_flash");
  assert.equal(dienstNormalisiert("openai"), "openai");
  assert.equal(dienstArt("gemini_pro"), "gemini");
  assert.equal(dienstName("gemini_flash"), "Gemini 3.8 Flash TTS");
  const rs = readFileSync(new URL("../src-tauri/src/config.rs", import.meta.url), "utf8");
  assert.ok(
    rs.includes('"gemini_flash" | "gemini_pro" => "gemini_38_flash"'),
    "Rust dienst_normalisiert weicht ab",
  );
});

pruefe("Liste deckt sich mit Rust dienst_art", () => {
  const rs = readFileSync(new URL("../src-tauri/src/config.rs", import.meta.url), "utf8");
  const a = rs.indexOf("pub fn dienst_art");
  const block = rs.slice(a, rs.indexOf("\n}\n", a));
  const rust = [...block.matchAll(/"([a-z0-9_]+)" => DienstArt::(\w+)/g)].map(
    ([, wert, art]) => [wert, art.toLowerCase()],
  );
  assert.ok(rust.length >= 7, "Rust-Liste nicht gefunden");
  const js = DIENSTE.filter((d) => d.wert !== "azure").map((d) => [d.wert, d.art]);
  assert.deepEqual(js, rust);
});

console.log(`Alle ${n} grün.`);
