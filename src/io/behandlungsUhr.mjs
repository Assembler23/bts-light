/** Behandlungsuhr am Zähl-Tablet (Spec
 *  `docs/features/verletzungsunterbrechung.md`, ADR 0095).
 *
 *  Nach ✚ zählt eine Uhr die Behandlung hoch; nach der Richtzeit wird sie
 *  rot und zählt weiter — die Pause endet nie von selbst. Hier steht nur die
 *  reine Rechnung:
 *  - Uhrtext `m:ss`, Minuten laufen über 59 hinaus („75:03");
 *  - „überschritten" ab genau der Richtzeit;
 *  - die Richtzeit aus dem `MatchBrief` (`injuryGuideSecs`), mit Rückfall
 *    auf 300 s, wenn ein alter Host sie nicht kennt;
 *  - die Fähigkeitsmeldung: Nur ein Host mit `injuryGuideSecs > 0` kennt die
 *    Behandlungs-Darstellung auf dem Zettel — nur dann erzeugt das Tablet
 *    automatische Behandlungsereignisse (ein alter Host druckte sie sonst bei
 *    Spieler „A1").
 *
 *  Kanonische Fassung. `tablet.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam; die
 *  Drift prüft `scripts/test-inline-kopien.mjs`.
 */

/** Richtzeit, wenn der Host keine meldet — derselbe Wert wie
 *  `RICHTZEIT_STANDARD_S` im Rust-Kern. */
export const RICHTZEIT_STANDARD_S = 300;
/** Bereich wie `RICHTZEIT_MIN_S`/`RICHTZEIT_MAX_S` im Rust-Kern. */
export const RICHTZEIT_MIN_S = 60;
export const RICHTZEIT_MAX_S = 1800;

/** Kennt der Host die Behandlungs-Darstellung (ADR 0095)? */
export function hostKenntBehandlung(brief) {
  const s = brief ? brief.injuryGuideSecs : undefined;
  return typeof s === "number" && Number.isFinite(s) && s > 0;
}

/** Richtzeit in Sekunden aus dem Brief — Fremdwerte (fehlt, 0, negativ,
 *  keine Zahl) ergeben den Standard, alles andere wird geklemmt. */
export function richtzeitAusBrief(brief) {
  if (!hostKenntBehandlung(brief)) return RICHTZEIT_STANDARD_S;
  const s = Math.round(brief.injuryGuideSecs);
  return Math.min(RICHTZEIT_MAX_S, Math.max(RICHTZEIT_MIN_S, s));
}

/** Sekunden als `m:ss`. */
export function uhrText(sekunden) {
  const s = Math.max(0, Math.floor(Number(sekunden) || 0));
  return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
}

/** Stand der Uhr — `null` ohne Startzeit (alter Stand: nur „✚", keine Uhr).
 *  `jetzt` und `startedAt` in Server-Zeit (ms), `richtSek` die beim Beginn
 *  eingefrorene Richtzeit. Eine Startzeit in der Zukunft (Uhrensprung)
 *  zählt als 0:00 statt rückwärts. */
export function behandlungsUhr(jetzt, startedAt, richtSek) {
  if (typeof startedAt !== "number" || !Number.isFinite(startedAt) || startedAt <= 0) return null;
  const sekunden = Math.max(0, Math.floor((jetzt - startedAt) / 1000));
  return { sekunden, text: uhrText(sekunden), ueberschritten: sekunden >= richtSek };
}
