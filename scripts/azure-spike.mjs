// Azure-TTS-Spike: vergleicht für harte Namen die Aussprache **roh** (deutscher
// Default) vs. **`<lang>` nativ** über mehrere mehrsprachige Stimmen und erzeugt
// MP3s zum Gegenhören. Entscheidungsgrundlage für den Hybrid-Plan
// (docs/features/name-pronunciation-plan.md, Phase 0).
//
// Voraussetzung: Azure Speech-Ressource (Key + Region, Region West Europe empf.).
// Aufruf:
//   AZURE_SPEECH_KEY=DEIN_KEY AZURE_SPEECH_REGION=westeurope node scripts/azure-spike.mjs
// Ergebnis: spike-out/*.mp3  →  je Name "<stimme>_<name>_roh.mp3" vs "..._lang.mp3"
//           und "full_<stimme>.mp3" (komplette Beispiel-Ansage mit <lang>).
//
// Achten auf: vietnamesische TÖNE (Nguyễn Thị Hồng, Phạm Thị Hồng Thu) — das ist
// der eigentliche Test. Mandarin + europäische Namen sind meist unkritisch.
import { mkdirSync, writeFileSync } from "node:fs";

const KEY = process.env.AZURE_SPEECH_KEY;
const REGION = process.env.AZURE_SPEECH_REGION || "westeurope";
if (!KEY) {
  console.error("AZURE_SPEECH_KEY fehlt (AZURE_SPEECH_KEY=… node scripts/azure-spike.mjs)");
  process.exit(1);
}

// 2–3 mehrsprachige Stimmen gegenhören.
const VOICES = [
  "de-DE-SeraphinaMultilingualNeural",
  "de-DE-FlorianMultilingualNeural",
];

// Harte Testfälle: Name + erkannte Sprache (Azure-Locale).
const NAMES = [
  { name: "Nguyễn Thị Hồng", locale: "vi-VN" },
  { name: "Phạm Thị Hồng Thu", locale: "vi-VN" },
  { name: "Zhang Zhixin", locale: "zh-CN" },
  { name: "Xu Yinsong", locale: "zh-CN" },
  { name: "García López", locale: "es-ES" },
  { name: "Lefèvre", locale: "fr-FR" },
  { name: "Wiśniewski", locale: "pl-PL" },
  { name: "Yılmaz", locale: "tr-TR" },
];

const esc = (s) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const speak = (voice, inner) =>
  `<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="de-DE">` +
  `<voice name="${voice}">${inner}</voice></speak>`;
const slug = (s) => s.normalize("NFD").replace(/[^a-zA-Z]/g, "").slice(0, 12);

async function synth(ssml, outfile) {
  const res = await fetch(
    `https://${REGION}.tts.speech.microsoft.com/cognitiveservices/v1`,
    {
      method: "POST",
      headers: {
        "Ocp-Apim-Subscription-Key": KEY,
        "Content-Type": "application/ssml+xml",
        "X-Microsoft-OutputFormat": "audio-24khz-48kbitrate-mono-mp3",
        "User-Agent": "bts-light-spike",
      },
      body: ssml,
    },
  );
  if (!res.ok) {
    const body = (await res.text()).slice(0, 200);
    console.error(`✗ ${outfile}: HTTP ${res.status} ${body}`);
    return;
  }
  const buf = Buffer.from(await res.arrayBuffer());
  writeFileSync(outfile, buf);
  console.log(`✓ ${outfile} (${buf.length} B)`);
}

mkdirSync("spike-out", { recursive: true });

for (const v of VOICES) {
  const vs = v.includes("Seraphina") ? "seraphina" : "florian";
  // Komplette Beispiel-Ansage mit <lang> (echtes Produktverhalten).
  const full =
    "Feld zwei. Herrendoppel. " +
    `<lang xml:lang="${NAMES[0].locale}">${esc(NAMES[0].name)}</lang>` +
    " gegen " +
    `<lang xml:lang="${NAMES[2].locale}">${esc(NAMES[2].name)}</lang>` +
    ".";
  await synth(speak(v, full), `spike-out/full_${vs}.mp3`);
  // Pro Name: roh vs. <lang> direkt vergleichbar.
  for (const n of NAMES) {
    await synth(speak(v, esc(n.name)), `spike-out/${vs}_${slug(n.name)}_roh.mp3`);
    await synth(
      speak(v, `<lang xml:lang="${n.locale}">${esc(n.name)}</lang>`),
      `spike-out/${vs}_${slug(n.name)}_lang.mp3`,
    );
  }
}
console.log("\nFertig. In spike-out/ je Name *_roh.mp3 vs *_lang.mp3 gegenhören.");
