// Besetzung in der Ansage (Spec `sr-ar-beim-aufruf`, ADR 0080) — eine Quelle
// für Feld- und Vorbereitungs-Ansage, Segment- und SSML-Weg.
//
// Beim Aufruf in Vorbereitung bringt der Turnier-PC die Namen von
// Schiedsrichter und Aufschlagrichter fertig im Ansage-Auftrag mit (ein
// LAN-Slave hat keine Schiedsrichter-Daten). Gesprochen werden sie nur beim
// Erst-Aufruf und nur, wenn die Besetzungs-Ansage an diesem Gerät an ist
// (ADR 0040).

/** Bereinigt eine Namensliste (leer/fehlend ⇒ leer). */
function sauber(namen) {
  return (namen ?? []).map((n) => String(n).trim()).filter(Boolean);
}

/**
 * Die SR/AR-Sätze: „Schiedsrichter: X." / „Aufschlagrichter: Y.".
 * @param {string[] | undefined} sr
 * @param {string[] | undefined} ar
 * @param {"de" | "en"} lang
 * @returns {string[]}
 */
export function besetzungSegmente(sr, ar, lang) {
  const out = [];
  const s = sauber(sr);
  const a = sauber(ar);
  if (s.length > 0) out.push(`${lang === "de" ? "Schiedsrichter" : "Umpire"}: ${s.join(" / ")}.`);
  if (a.length > 0) {
    out.push(`${lang === "de" ? "Aufschlagrichter" : "Service judge"}: ${a.join(" / ")}.`);
  }
  return out;
}

/**
 * Besetzung für die FELD-Ansage: die SR/AR-Sätze und danach noch einmal
 * das Feld, damit die Ansage immer mit dem Feld endet (Nutzerwunsch
 * 14.11.2026 — nach den Namen hörte man sonst zuletzt einen Schiedsrichter,
 * nicht wohin es geht). Ohne Besetzung bleibt es leer; das Feld steht dann
 * ohnehin schon am Ende.
 * @param {string[] | undefined} sr
 * @param {string[] | undefined} ar
 * @param {"de" | "en"} lang
 * @param {string} feld rohe Feld-Phrase, z. B. „Feld 3"
 * @returns {string[]}
 */
export function besetzungMitFeld(sr, ar, lang, feld) {
  const out = besetzungSegmente(sr, ar, lang);
  if (out.length > 0 && feld.trim()) out.push(`${feld.trim()}.`);
  return out;
}

/**
 * Namen aus einem Aufruf-Auftrag für die Ansage — gefiltert über den
 * Besetzungs-Schalter dieses Geräts. Nachrufe (Stufe 2/3) nennen nie eine
 * Besetzung.
 * @param {{ kind: string, sr?: string[], ar?: string[] }} job
 * @param {boolean} besetzungAnsagen `announce_umpire !== false`
 * @returns {{ umpireNames?: string[], serviceJudgeNames?: string[] }}
 */
export function besetzungAusAuftrag(job, besetzungAnsagen) {
  if (job.kind !== "meetingpoint_call" || !besetzungAnsagen) return {};
  return { umpireNames: sauber(job.sr), serviceJudgeNames: sauber(job.ar) };
}
