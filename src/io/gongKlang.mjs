// Der Gong vor einer Ansage — Klang allein, ohne Warten und ohne Sprache.
//
// Eine Stelle für den Klang, weil ihn zwei Orte erzeugen: der Lautsprecher
// am Turnier-PC (`announcer.ts`) und die Ansage-Seite im Browser
// (`assets/ansage.html`, Spec `ansage-im-browser`). Die Seite trägt eine
// Inline-Kopie zwischen den Markern `<gongKlang.mjs>`; `scripts/test-gong-klang.mjs`
// hält sie wortgleich. So klingt der Gong in der Halle wie am PC.

/**
 * Plant den Gong auf dem AudioContext ein und liefert den zuletzt stoppenden
 * Oszillator samt geplantem Ende (Sekunden der Audio-Uhr) — daran hängt der
 * Aufrufer sein Warten, damit die Sprache nicht in den Nachklang einsetzt.
 *
 * `match` = Spielaufruf: tiefer, zweitöniger ABSTEIGENDER Sinus-Gong
 * (A5 → D5), wie ein Hotel-Gong. `info` = Info/Freitext: drei helle,
 * perlende Töne (C-Dur-Dreiklang aufsteigend, Dreieck-Welle) — klar anders
 * in Tonzahl, Klangfarbe und Bewegung.
 *
 * @param {AudioContext} ctx
 * @param {"match" | "info"} kind
 * @returns {{ letzter: OscillatorNode, endeSec: number }}
 */
export function gongPlanen(ctx, kind) {
  const now = ctx.currentTime;
  if (kind === "info") {
    const notes = [523.25, 659.25, 783.99]; // C5 – E5 – G5
    const step = 0.15;
    let letzter = null;
    let endeSec = now;
    for (let i = 0; i < notes.length; i++) {
      const t = now + i * step;
      const g = ctx.createGain();
      g.gain.setValueAtTime(0.0001, t);
      g.gain.exponentialRampToValueAtTime(0.32, t + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, t + 0.32);
      g.connect(ctx.destination);
      const o = ctx.createOscillator();
      o.type = "triangle";
      o.frequency.value = notes[i];
      o.connect(g);
      o.start(t);
      const stopAt = t + 0.38;
      o.stop(stopAt);
      letzter = o;
      endeSec = stopAt;
    }
    return { letzter, endeSec };
  }

  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(0.4, now + 0.05);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + 1.2);
  gain.connect(ctx.destination);

  const o1 = ctx.createOscillator();
  o1.type = "sine";
  o1.frequency.value = 880;
  o1.connect(gain);
  o1.start(now);
  o1.stop(now + 0.6);

  const o2 = ctx.createOscillator();
  o2.type = "sine";
  o2.frequency.value = 587.33;
  o2.connect(gain);
  o2.start(now + 0.18);
  o2.stop(now + 1.1);

  // `o2.stop` bei ~1,1 s beendet den Ton hart; die Gain-Rampe bis 1,2 s ist
  // danach wirkungslos. 1,2 s dient nur der konservativen Fallback-Frist.
  return { letzter: o2, endeSec: now + 1.2 };
}
