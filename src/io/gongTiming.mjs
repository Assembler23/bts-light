// Reine Auflöse-Logik für das Gong-Ende — bewusst OHNE Web-Audio-Bezug, damit
// sie in Node getestet werden kann (scripts/test-gong-timing.mjs). announcer.ts
// koppelt sie an einen echten OscillatorNode; hier steht nur das Timing-Race.

/** Kleine Atempause zwischen Gong-Ende und Sprachbeginn (ms) — klingt
 *  natürlicher und puffert Restjitter der Audio-/OS-Uhr. */
export const GONG_BREATH_MS = 150;

/**
 * Löst auf, sobald ENTWEDER das echte Audio-Ende gemeldet wird (`subscribeEnded`
 * ruft den übergebenen Callback) ODER der Fallback-Timer feuert — je nachdem,
 * was zuerst kommt. Danach folgt `breathMs` Atempause. Ein `done`-Flag
 * verhindert doppelte Auflösung, wenn beide Signale eintreffen.
 *
 * Der Fallback ist die Absicherung, falls `onended` in WebView2 ausnahmsweise
 * ausbleibt: die Ansage-Queue darf nie hängen. Er läuft daher IMMER mit (nicht
 * bedingt), nicht nur „wenn onended fehlt".
 *
 * @param {object} p
 * @param {(cb: () => void) => void} p.subscribeEnded  meldet das echte Audio-Ende
 * @param {number} p.fallbackMs                        Fallback-Frist (ms)
 * @param {number} [p.breathMs]                        Atempause (Default GONG_BREATH_MS)
 * @param {(cb: () => void, ms: number) => unknown} [p.setTimeoutFn]  injizierbar für Tests
 * @returns {Promise<void>}
 */
export function gongResolveRace({
  subscribeEnded,
  fallbackMs,
  breathMs = GONG_BREATH_MS,
  setTimeoutFn = setTimeout,
}) {
  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      setTimeoutFn(resolve, breathMs);
    };
    subscribeEnded(finish);
    setTimeoutFn(finish, fallbackMs);
  });
}
