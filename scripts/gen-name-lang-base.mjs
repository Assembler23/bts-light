// Generiert src/io/nameLangBase.ts aus data/name-lists/namen_lexicon_<locale>.xml.
//
// Zweck: Name → Herkunftssprache für den Azure-`<lang>`-Pfad (Hybrid-Plan,
// docs/features/name-pronunciation-plan.md). Pro Sprachdatei sind die Grapheme
// muttersprachliche Namen → sichere Klassifikations-Treffer.
//
// Regeln:
// - Schlüssel werden wie `transliterate.fold()` normalisiert (NFD, Diakritika
//   weg, ı/İ→i, lowercase) → identisches Matching zur Laufzeit.
// - **Mehrdeutige Namen** (in mehreren Sprachdateien) werden WEGGELASSEN — bei
//   Unsicherheit lieber kein `<lang>` (deutscher Default) als souverän falsch.
// - de-DE wird NICHT genutzt (gemischte Herkunft: CJK/VN → laufen über die
//   CN/VN-Trigger in transliterate.ts).
//
// Aufruf:  node scripts/gen-name-lang-base.mjs
import { readFileSync, writeFileSync, readdirSync } from "node:fs";

const DIR = "data/name-lists";
// Dateiname-Locale → interne NameLang.
const FILE_LANG = {
  "es-ES": "es", "fr-FR": "fr", "pl-PL": "pl",
  "tr-TR": "tr", "ms-MY": "ms", "en-IN": "in",
};

// Muss EXAKT der Laufzeit-Faltung in transliterate.detectNameLang entsprechen
// (inkl. đ→d), sonst weichen die Schlüssel ab und der Lookup verfehlt.
function fold(s) {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[ıİ]/g, "i")
    .replace(/[đĐ]/g, "d")
    .trim()
    .toLowerCase();
}

// name → Set<lang> (über alle Dateien), um Mehrdeutigkeiten zu erkennen.
const langsByName = new Map();

for (const file of readdirSync(DIR).sort()) {
  const m = file.match(/namen_lexicon_(.+)\.xml$/);
  if (!m || !FILE_LANG[m[1]]) continue;
  const lang = FILE_LANG[m[1]];
  const xml = readFileSync(`${DIR}/${file}`, "utf8");
  for (const g of xml.matchAll(/<grapheme>([\s\S]*?)<\/grapheme>/g)) {
    const key = fold(g[1]);
    if (!key) continue;
    if (!langsByName.has(key)) langsByName.set(key, new Set());
    langsByName.get(key).add(lang);
  }
}

// Nur eindeutige Namen übernehmen; mehrdeutige zählen + verwerfen.
const entries = [];
let ambiguous = 0;
for (const [name, langs] of langsByName) {
  if (langs.size === 1) entries.push([name, [...langs][0]]);
  else ambiguous++;
}
entries.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));

const body = entries.map(([n, l]) => `  [${JSON.stringify(n)}, ${JSON.stringify(l)}],`).join("\n");
const out = `// AUTO-GENERIERT von scripts/gen-name-lang-base.mjs — nicht von Hand editieren.
// Quelle: data/name-lists/namen_lexicon_<locale>.xml (kuratierte Lexika).
// Gefalteter Name → Herkunftssprache für den Azure-\`<lang>\`-Pfad. Mehrdeutige
// Namen (in mehreren Sprachen) sind bewusst NICHT enthalten (deutscher Default).
import type { NameLang } from "./transliterate";

export const NAME_LANG_BASE: Map<string, NameLang> = new Map([
${body}
]);
`;

writeFileSync("src/io/nameLangBase.ts", out);
console.log(`nameLangBase.ts: ${entries.length} eindeutige Namen, ${ambiguous} mehrdeutige verworfen.`);
