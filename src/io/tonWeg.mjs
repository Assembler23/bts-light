// Reine Logik des Ansage-Tonwegs (Spec `ansage-ausgabegeraet`, ADR 0065):
// Gong und Stimme spielt der Rust-Kern; announcer.ts entscheidet hier, wie
// es bei einem Kern-Fehler weitergeht und wie lange es höchstens wartet.
// Bewusst ohne Tauri- und Web-Audio-Bezug, damit Node es testen kann
// (scripts/test-ton-weg.mjs).

/** Fehlercodes, die der Kern als "<code>: <Text>" meldet. */
const BEKANNTE_CODES = new Set([
  "nicht_unterstuetzt",
  "kein_geraet",
  "keine_stimme",
  "abgebrochen",
  "ungueltig",
  "intern",
]);

/**
 * Liest den Fehlercode aus einer Kern-Antwort ("kein_geraet: …"). Alles
 * Unbekannte zählt als "intern" — lieber ein Rückfall-Banner zu viel als
 * ein stiller Fehler.
 * @param {unknown} err
 * @returns {string}
 */
export function fehlerCode(err) {
  const text = err instanceof Error ? err.message : err == null ? "" : String(err);
  const code = text.split(":")[0].trim();
  return BEKANNTE_CODES.has(code) ? code : "intern";
}

/**
 * Welches Banner ein Fehler auslöst. `null` = keins: Unter Linux gibt es den
 * Kern-Weg nicht (heutiges Verhalten, kein Fehler), und ein Abbruch ist
 * gewollt.
 * @param {string} code  Fehlercode oder "frist"
 * („geraet_fehlt" meldet announcer.ts selbst: Das ist kein Fehler, das
 * Stück lief über den Ersatz.)
 * @returns {"kein_geraet" | "keine_stimme" | "rueckfall" | null}
 */
export function bannerFuer(code) {
  if (code === "nicht_unterstuetzt" || code === "abgebrochen") return null;
  if (code === "kein_geraet") return "kein_geraet";
  if (code === "keine_stimme") return "keine_stimme";
  return "rueckfall";
}

/** Sockel jeder Frist: Gerät öffnen (bis 3 Versuche à 300 ms), Abriss
 *  erkennen (bis 2 s Stillstand), Rand und Nachlauf. */
const SOCKEL_MS = 5000;

/**
 * Höchste Wartezeit auf ein PCM-Stück (Gong, Azure-Stimme). Deckt EINE
 * Wiederholung ab: Reißt das Gerät mitten im Stück ab, spielt der Kern es
 * auf dem neuen Gerät von vorn (D8) — eine Frist von nur „Dauer + Sockel"
 * bräche genau diese Rettung ab. Danach bricht announcer.ts hart ab und die
 * Kette läuft weiter.
 * @param {number} samples
 * @param {number} rate
 */
export function pcmFristMs(samples, rate) {
  const dauer = rate > 0 ? Math.ceil((samples / rate) * 1000) : 0;
  return 2 * dauer + SOCKEL_MS;
}

/** Geschätzte Sprechzeit je Zeichen bei Rate 0,8 (wie die frühere
 *  Web-Speech-Frist) und Fuge zwischen Abschnitten im Kern. */
const MS_JE_ZEICHEN = 140;
const FUGE_MS = 300;
const RAND_MS = 170;

/**
 * Höchste Wartezeit auf die Windows-Stimme: geschätzte Sprechzeit (langsamer
 * bei kleiner Rate, plus Fugen) zweimal — wegen der möglichen Wiederholung —
 * plus Sockel für Synthese und Öffnen.
 * @param {string[]} segmente
 * @param {number} rate
 */
export function stimmeFristMs(segmente, rate) {
  const r = Number.isFinite(rate) && rate > 0 ? Math.max(0.5, rate) : 0.8;
  const zeichen = segmente.reduce((n, s) => n + String(s || "").trim().length, 0);
  const schaetz = Math.ceil(
    zeichen * MS_JE_ZEICHEN * (0.8 / r) + segmente.length * FUGE_MS + RAND_MS,
  );
  return 2 * schaetz + SOCKEL_MS;
}

/**
 * Teilt zu lange Abschnitte, bevor sie an den Kern gehen (der nimmt je
 * Abschnitt höchstens `max` Zeichen und kappte den Rest still). Geschnitten
 * wird am letzten Satzende, sonst am letzten Leerzeichen, sonst hart. Leere
 * Abschnitte fallen weg.
 * @param {string[]} segmente
 * @param {number} max
 * @returns {string[]}
 */
export function abschnitteTeilen(segmente, max) {
  const out = [];
  for (const roh of segmente) {
    let rest = String(roh || "").trim();
    while (rest.length > max) {
      const kopf = rest.slice(0, max);
      let schnitt = Math.max(
        kopf.lastIndexOf(". "),
        kopf.lastIndexOf("! "),
        kopf.lastIndexOf("? "),
      );
      schnitt = schnitt > 0 ? schnitt + 1 : kopf.lastIndexOf(" ");
      if (schnitt <= 0) schnitt = max;
      out.push(rest.slice(0, schnitt).trim());
      rest = rest.slice(schnitt).trim();
    }
    if (rest) out.push(rest);
  }
  return out;
}

/**
 * Wartet höchstens `ms` auf `promise`. Liefert genau einmal:
 * `{ok: true, wert}`, `{ok: false, code}` (Kern-Fehler) oder
 * `{ok: false, code: "frist"}`. Ein Ergebnis nach der Frist wird ignoriert —
 * die Ansage-Kette darf nie hängen.
 * @param {Promise<unknown>} promise
 * @param {number} ms
 * @param {{setTimeoutFn?: Function, clearTimeoutFn?: Function}} [uhr]
 */
export function mitFrist(promise, ms, uhr = {}) {
  const setT = uhr.setTimeoutFn || setTimeout;
  const clearT = uhr.clearTimeoutFn || clearTimeout;
  return new Promise((resolve) => {
    let fertig = false;
    const ende = (r) => {
      if (fertig) return;
      fertig = true;
      clearT(timer);
      resolve(r);
    };
    const timer = setT(() => ende({ ok: false, code: "frist" }), ms);
    promise.then(
      (wert) => ende({ ok: true, wert }),
      (err) => ende({ ok: false, code: fehlerCode(err) }),
    );
  });
}
