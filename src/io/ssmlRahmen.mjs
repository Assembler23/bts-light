// Rahmen, Tempo und Pausen für das Azure-SSML der Ansagen.
//
// Warum: Bis v0.9.332 sprach Azure jede Ansage im Normaltempo — der Regler
// „Tempo" (Standard 0,8) wirkte nur auf Windows-/Browser-Stimme. Und die
// Blöcke (Klasse | Namen | „gegen" | Feld) standen nur durch Punkte getrennt
// hintereinander; im Hallenhall verschwammen die Übergänge, Wörter wurden
// „verschluckt" — besonders an den Sprachwechseln der Namen (`<lang>`).
//
// Verschachtelung nach Azure-Regeln: `<prosody>` darf KEIN `<lang>`
// enthalten, `<lang>` aber ein `<prosody>`. Deshalb bekommt jedes Stück
// zwischen den `<lang>`-Spans sein eigenes `<prosody>`, und der Inhalt jedes
// `<lang>`-Spans ebenfalls — nie ein `<prosody>` um alles.

/** Pause zwischen zwei Blöcken der Ansage (Klasse, Paarung, Feld …). */
export const BLOCK_PAUSE_MS = 350;
/** Kurze Pause vor und nach einem Namen in fremder Sprache (`<lang>`). */
export const LANG_PAUSE_MS = 120;

const blockPause = `<break time="${BLOCK_PAUSE_MS}ms"/>`;
const langPause = `<break time="${LANG_PAUSE_MS}ms"/>`;

/** Tempo 0,5–1,5 → relativer Azure-Wert („-20%"); `null` bei Normaltempo. */
export function tempoProzent(rate) {
  if (rate == null || !Number.isFinite(rate)) return null;
  const r = Math.min(1.5, Math.max(0.5, rate));
  const p = Math.round((r - 1) * 100);
  if (p === 0) return null;
  return `${p > 0 ? "+" : ""}${p}%`;
}

const LANG_SPAN = /<lang xml:lang="[^"]*">[\s\S]*?<\/lang>/g;

/** Fremdsprachige Namen mit kurzer Pause absetzen. */
export function langAbsetzen(inner) {
  return inner.replace(LANG_SPAN, (span) => `${langPause}${span}${langPause}`);
}

/** Tempo in ein fertiges SSML-Inneres einziehen, `<lang>`-sicher. */
export function mitTempo(inner, rate) {
  const p = tempoProzent(rate);
  if (!p || !inner) return inner;
  const auf = `<prosody rate="${p}">`;
  const wrap = (s) => (s.trim() ? `${auf}${s}</prosody>` : s);
  let out = "";
  let last = 0;
  for (const m of inner.matchAll(LANG_SPAN)) {
    out += wrap(inner.slice(last, m.index));
    const span = m[0];
    const open = span.indexOf(">") + 1;
    const close = span.lastIndexOf("</lang>");
    out += span.slice(0, open) + wrap(span.slice(open, close)) + "</lang>";
    last = m.index + span.length;
  }
  out += wrap(inner.slice(last));
  return out;
}

function xmlAttr(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Das ganze `<speak>`-Dokument. `teile` sind fertige (escapte) SSML-Blöcke;
 * zwischen ihnen steht eine echte Pause, fremde Namen werden abgesetzt, und
 * das Tempo gilt für alles.
 */
export function ssmlDokument(lang, voice, teile, rate) {
  const speakLang = lang === "de" ? "de-DE" : "en-US";
  const inner = mitTempo(
    langAbsetzen(teile.filter((t) => t && t.trim()).join(blockPause)),
    rate,
  );
  return (
    `<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="${speakLang}">` +
    `<voice name="${xmlAttr(voice)}">${inner}</voice></speak>`
  );
}
