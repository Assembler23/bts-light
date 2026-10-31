// Wie die Klasse hinter der Disziplin gesprochen wird („Herreneinzel A").
//
// Mit nur einem Leerzeichen dazwischen ziehen die Stimmen das Kürzel an das
// Wort heran — in der Halle klang es wie „Herreneinzela" (Feldbericht
// 30.10.2026). Deshalb wird die Klasse beim SPRECHEN abgesetzt; die Anzeige
// (`disciplineWithClass`) bleibt „Herreneinzel A".

/** Pause zwischen Disziplin und Klasse im Azure-SSML. */
export const KLASSE_PAUSE_MS = 250;

/** Text für die Gerätestimme (Web Speech): Das Komma ist die Pause, die
 *  jede Stimme macht — ein Bindestrich wird oft einfach mitgezogen. */
export function klasseText(disziplin, klasse) {
  const d = String(disziplin || "").trim();
  const k = String(klasse || "").trim();
  if (!d) return "";
  return k ? d + ", " + k : d;
}

function xml(s) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/** SSML-Stück für Azure (escapt selbst): echte Pause statt Komma, und ein
 *  reines Buchstaben-Kürzel wird buchstabiert — sonst liest eine englische
 *  Stimme „A" als Artikel. „U15" oder „B2" bleiben, wie sie sind. */
export function klasseSsml(disziplin, klasse) {
  const d = String(disziplin || "").trim();
  const k = String(klasse || "").trim();
  if (!d) return "";
  if (!k) return xml(d);
  const kern = /^[A-Za-z]{1,2}$/.test(k)
    ? '<say-as interpret-as="characters">' + k + "</say-as>"
    : xml(k);
  return xml(d) + '<break time="' + KLASSE_PAUSE_MS + 'ms"/>' + kern;
}
