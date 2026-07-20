// Testet die reine Gong-Auflöse-Logik (src/io/gongTiming.mjs) — DASSELBE Modul,
// das announcer.ts an den echten OscillatorNode koppelt (kein Par-Kopie-Drift).
// Web Audio gibt es in Node nicht; getestet wird nur das Timing-Race:
//  A) echtes Audio-Ende (onended) zuerst  → Auflösung ~Atempause danach
//  B) onended bleibt aus                   → Fallback-Frist greift, dann Atempause
//  C) done-Guard: genau EINE Auflösung, auch wenn beide Signale feuern
//
// Deterministisch über einen injizierten Fake-Timer (virtuelle Uhr), damit der
// Test ohne echte Wartezeit läuft.
import { gongResolveRace, GONG_BREATH_MS } from "../src/io/gongTiming.mjs";

let failures = 0;
function check(name, cond) {
  if (!cond) {
    console.error(`✗ ${name}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// Minimaler Fake-Timer: sammelt (Callback, Fälligkeit) und lässt die virtuelle
// Uhr per advance() vorlaufen. Reihenfolge nach Fälligkeit, dann Einfügereihe.
function makeClock() {
  let nowMs = 0;
  let seq = 0;
  const timers = [];
  const setTimeoutFn = (cb, ms) => {
    timers.push({ at: nowMs + ms, seq: seq++, cb });
  };
  const advance = (ms) => {
    const until = nowMs + ms;
    for (;;) {
      const due = timers
        .filter((t) => t.at <= until)
        .sort((a, b) => a.at - b.at || a.seq - b.seq)[0];
      if (!due) break;
      timers.splice(timers.indexOf(due), 1);
      nowMs = due.at;
      due.cb();
    }
    nowMs = until;
  };
  return { setTimeoutFn, advance, now: () => nowMs };
}

// A) onended zuerst: Ende bei 100 ms, Fallback erst bei 1000 ms.
async function pathEndedFirst() {
  const clock = makeClock();
  let endedCb = null;
  let resolvedAt = -1;
  const p = gongResolveRace({
    subscribeEnded: (cb) => {
      endedCb = cb;
    },
    fallbackMs: 1000,
    setTimeoutFn: clock.setTimeoutFn,
  }).then(() => {
    resolvedAt = clock.now();
  });
  clock.advance(100); // echtes Audio-Ende meldet sich
  endedCb();
  clock.advance(GONG_BREATH_MS); // Atempause abwarten
  await p;
  check(
    "A: onended zuerst → Auflösung nach Ende + Atempause",
    resolvedAt === 100 + GONG_BREATH_MS,
  );
}

// B) onended bleibt aus: nur der Fallback bei 800 ms rettet die Auflösung.
async function pathFallbackOnly() {
  const clock = makeClock();
  let resolvedAt = -1;
  const p = gongResolveRace({
    subscribeEnded: () => {
      /* onended feuert nie (WebView2-Aussetzer) */
    },
    fallbackMs: 800,
    setTimeoutFn: clock.setTimeoutFn,
  }).then(() => {
    resolvedAt = clock.now();
  });
  clock.advance(800 + GONG_BREATH_MS);
  await p;
  check(
    "B: onended aus → Fallback + Atempause löst auf",
    resolvedAt === 800 + GONG_BREATH_MS,
  );
}

// C) done-Guard: onended zuerst, danach feuert AUCH der Fallback → nur eine
//    Auflösung, kein Doppel-Resolve/keine zweite Atempause-Wirkung.
async function guardSingleResolve() {
  const clock = makeClock();
  let endedCb = null;
  let resolveCount = 0;
  const p = gongResolveRace({
    subscribeEnded: (cb) => {
      endedCb = cb;
    },
    fallbackMs: 500,
    setTimeoutFn: clock.setTimeoutFn,
  }).then(() => {
    resolveCount++;
  });
  clock.advance(100);
  endedCb(); // Ende zuerst
  clock.advance(1000); // Fallback (500 ms) würde jetzt auch feuern
  await p;
  check("C: done-Guard → genau eine Auflösung", resolveCount === 1);
}

await pathEndedFirst();
await pathFallbackOnly();
await guardSingleResolve();

if (failures > 0) {
  console.error(`\n✗ Gong-Timing-Test: ${failures} Fehler.`);
  process.exit(1);
}
console.log(
  "\n✓ Gong-Timing-Test: onended-Pfad, Fallback-Pfad, done-Guard ok.",
);
