#!/usr/bin/env node
// Lastskript der Anzeige-Strecke (Spec `docs/features/monitor-livestand-push.md`,
// Etappe S0 b). Erzeugt die Last eines vollen Turniers gegen einen laufenden
// Turnier-PC oder gegen den Cloud-Relay und misst, was dabei über die Leitung
// geht — der Vorher-Wert, ohne den laut Spec keine der folgenden Etappen
// begonnen wird.
//
// Bewusst nur Node-Bordmittel (globales `fetch`, globales `WebSocket`, ab
// Node 22) und **kein CI-Schritt**: Das hier braucht einen echten Server mit
// echten Matches, keinen Testlauf.
//
// Aufruf (bts-light muss laufen, BTP verbunden, Felder belegt):
//
//   node scripts/last-monitor.mjs --base http://192.168.1.5:8088/
//   node scripts/last-monitor.mjs --base https://badhub.de/bts-relay/<install-id>/
//
// Parameter (alle optional):
//   --tablets N          zählende Tablets            (Vorgabe: 20)
//   --uebersichten N     Feld-Übersichten            (Vorgabe: 20)
//   --court-monitore N   feste Court-Monitore        (Vorgabe: 0)
//   --dauer N            Messdauer in Sekunden       (Vorgabe: 60)
//   --punkte N           Punkte je Minute je Tablet  (Vorgabe: 12 = alle 5 s)
//   --trocken            nur verbinden und lauschen, nichts senden
//
// Was gemessen wird: Abrufe und Bytes je Anzeigenart, die Latenz vom
// gesendeten Punkt bis zu seinem Erscheinen in einer Übersichts-Antwort
// (p50/p95) und — falls erreichbar — der Zählerstand des Servers aus
// `/debug/perf` daneben.

const args = process.argv.slice(2);

/** Wert eines `--name wert`-Paares. */
function argWert(name, vorgabe) {
  const i = args.indexOf(`--${name}`);
  if (i < 0 || i + 1 >= args.length) return vorgabe;
  return args[i + 1];
}
/** Ist `--name` gesetzt? */
function argFlag(name) {
  return args.includes(`--${name}`);
}
function argZahl(name, vorgabe) {
  const n = Number(argWert(name, vorgabe));
  return Number.isFinite(n) && n >= 0 ? n : vorgabe;
}

const BASE = String(argWert("base", "http://127.0.0.1:8088/")).replace(/\/*$/, "/");
const TABLETS = argZahl("tablets", 20);
const UEBERSICHTEN = argZahl("uebersichten", 20);
const COURT_MONITORE = argZahl("court-monitore", 0);
const DAUER_S = argZahl("dauer", 60);
const PUNKTE_PRO_MIN = argZahl("punkte", 12);
const TROCKEN = argFlag("trocken");

// Aus dem Original übernommen, sonst misst das Skript eine andere Anzeige als
// die, die im Feld hängt (overview.html: COALESCE_MS, FALLBACK_MS).
const COALESCE_MS = 60;
const FALLBACK_MS = 250;
const PUSH_FRESH_MS = 1200;

function wsUrl(pfad) {
  return BASE.replace(/^http/, "ws") + pfad;
}

const stat = {
  healthAbrufe: 0,
  healthBytes: 0,
  healthFehler: 0,
  courtAbrufe: 0,
  courtBytes: 0,
  nudges: 0,
  punkte: 0,
  latenzen: [],
};

/** Perzentil eines Zahlen-Arrays (nächster Rang, ohne Interpolation). */
function perzentil(werte, p) {
  if (werte.length === 0) return 0;
  const s = [...werte].sort((a, b) => a - b);
  const rang = Math.max(1, Math.ceil((s.length * p) / 100));
  return s[rang - 1];
}

const schlaf = (ms) => new Promise((r) => setTimeout(r, ms));

/** Letzter Satzstand eines Feldes aus der Übersichts-Antwort. */
function letzterSatz(court) {
  const sets = Array.isArray(court && court.sets) ? court.sets : [];
  if (sets.length === 0) return null;
  const s = sets[sets.length - 1];
  if (Array.isArray(s)) return { a: s[0] | 0, b: s[1] | 0 };
  if (s && typeof s === "object") return { a: s.a | 0, b: s.b | 0 };
  return null;
}

// Was ein Tablet zuletzt gesendet hat: CourtID → {a, b, ts}. Die Übersichten
// vergleichen jede Antwort dagegen und schreiben die Latenz fort, sobald der
// Wert erstmals sichtbar ist. Genau EINE Messung je gesendetem Punkt.
const offen = new Map();

function pruefeLatenz(courts) {
  const jetzt = Date.now();
  for (const c of courts || []) {
    const erwartet = offen.get(c.court_id);
    if (!erwartet) continue;
    const satz = letzterSatz(c);
    if (!satz) continue;
    if (satz.a === erwartet.a && satz.b === erwartet.b) {
      stat.latenzen.push(jetzt - erwartet.ts);
      offen.delete(c.court_id);
    }
  }
}

// ── Feld-Übersicht (overview.html) ────────────────────────────────────────
//
// Nudge-getriebener Fetch mit demselben Coalescing wie im Original, dazu der
// 250-ms-Fallback, wenn seit 1,2 s kein Nudge kam.
function starteUebersicht(nr) {
  let mwsOpen = false;
  let letzterNudge = 0;
  let laeuft = false;
  let nachschlag = false;
  let letzterStart = 0;
  let coalesceTimer = null;
  let quelle = "poll";
  const istMessend = nr === 0; // nur eine Übersicht prüft die Latenz

  async function hole() {
    if (laeuft) {
      nachschlag = true;
      return;
    }
    laeuft = true;
    const src = quelle;
    quelle = "poll";
    try {
      const r = await fetch(`${BASE}health?src=${src}`, { cache: "no-store" });
      const text = await r.text();
      stat.healthAbrufe++;
      stat.healthBytes += text.length;
      if (istMessend) {
        try {
          pruefeLatenz(JSON.parse(text).courts);
        } catch {
          /* unlesbare Antwort zählt als Fehler, nicht als Absturz */
        }
      }
    } catch {
      stat.healthFehler++;
    } finally {
      laeuft = false;
      if (nachschlag) {
        nachschlag = false;
        anfordern();
      }
    }
  }

  function anfordern() {
    const jetzt = Date.now();
    const warten = COALESCE_MS - (jetzt - letzterStart);
    if (warten <= 0) {
      letzterStart = jetzt;
      hole();
    } else if (!coalesceTimer) {
      coalesceTimer = setTimeout(() => {
        coalesceTimer = null;
        letzterStart = Date.now();
        hole();
      }, warten);
    }
  }

  function verbinde() {
    let ws;
    try {
      ws = new WebSocket(wsUrl("monitor-ws"));
    } catch {
      setTimeout(verbinde, 1000);
      return;
    }
    ws.onopen = () => {
      mwsOpen = true;
    };
    ws.onmessage = () => {
      stat.nudges++;
      letzterNudge = Date.now();
      quelle = "push";
      anfordern();
    };
    ws.onclose = () => {
      mwsOpen = false;
      setTimeout(verbinde, 1000);
    };
    ws.onerror = () => {};
  }

  verbinde();
  anfordern();
  return setInterval(() => {
    const frisch = mwsOpen && Date.now() - letzterNudge < PUSH_FRESH_MS;
    if (!frisch) anfordern();
  }, FALLBACK_MS);
}

// ── Fester Court-Monitor (monitor.html) ───────────────────────────────────
function starteCourtMonitor(courtId) {
  let mwsOpen = false;
  let letzterNudge = 0;
  let quelle = "poll";

  async function hole() {
    const src = quelle;
    quelle = "poll";
    try {
      const r = await fetch(`${BASE}court/${courtId}/state?src=${src}`, { cache: "no-store" });
      const text = await r.text();
      stat.courtAbrufe++;
      stat.courtBytes += text.length;
    } catch {
      stat.healthFehler++;
    }
  }

  function verbinde() {
    let ws;
    try {
      ws = new WebSocket(wsUrl(`monitor-ws?court=${courtId}`));
    } catch {
      setTimeout(verbinde, 1000);
      return;
    }
    ws.onopen = () => {
      mwsOpen = true;
    };
    ws.onmessage = () => {
      letzterNudge = Date.now();
      quelle = "push";
      hole();
    };
    ws.onclose = () => {
      mwsOpen = false;
      setTimeout(verbinde, 1000);
    };
    ws.onerror = () => {};
  }

  verbinde();
  hole();
  return setInterval(() => {
    const frisch = mwsOpen && Date.now() - letzterNudge < PUSH_FRESH_MS;
    if (!frisch) hole();
  }, FALLBACK_MS);
}

// ── Zählendes Tablet ──────────────────────────────────────────────────────
//
// Sendet je Punkt `score_update` UND `state_sync` — heute erzeugt jeder Punkt
// dadurch ZWEI Nudges, und genau das soll die Messung abbilden.
function starteTablet(court, index) {
  let ws = null;
  let bereit = false;
  let a = 0;
  let b = 0;

  function verbinde() {
    try {
      ws = new WebSocket(wsUrl("ws"));
    } catch {
      setTimeout(verbinde, 1000);
      return;
    }
    ws.onopen = () => {
      ws.send(
        JSON.stringify({
          type: "identify",
          courtId: court.court_id,
          courtLabel: court.court || `Feld ${court.court_id}`,
          deviceId: `last-monitor-${index}`,
        }),
      );
      bereit = true;
    };
    ws.onclose = () => {
      bereit = false;
      setTimeout(verbinde, 1000);
    };
    ws.onerror = () => {};
  }

  function punkt() {
    if (!bereit || !ws || ws.readyState !== 1 || TROCKEN) return;
    // Abwechselnd, damit kein Satz vorzeitig endet und der Stand realistisch
    // wächst; bei 20:20 zurück auf 0:0 (neuer Satz wäre eine andere Messung).
    if (a >= 19 && b >= 19) {
      a = 0;
      b = 0;
    } else if (a <= b) {
      a++;
    } else {
      b++;
    }
    ws.send(
      JSON.stringify({
        type: "score_update",
        scoreA: a,
        scoreB: b,
        setsHistory: [],
        matchId: court.match_id,
      }),
    );
    ws.send(
      JSON.stringify({
        type: "state_sync",
        state: JSON.stringify({ scoreA: a, scoreB: b, matchId: court.match_id }),
      }),
    );
    stat.punkte++;
    // Nur die jüngste Erwartung je Feld halten — eine ältere, die nie sichtbar
    // wurde, ist überholt und würde die Latenz verfälschen.
    offen.set(court.court_id, { a, b, ts: Date.now() });
  }

  verbinde();
  const abstand = Math.max(250, Math.round(60000 / Math.max(1, PUNKTE_PRO_MIN)));
  // Versetzt starten, sonst schlagen alle Tablets im selben Millisekundenfenster
  // auf und das Bild wäre unrealistisch spitz.
  setTimeout(() => setInterval(punkt, abstand), Math.round((abstand * index) / Math.max(1, TABLETS)));
  return ws;
}

// ── Lauf ──────────────────────────────────────────────────────────────────

async function main() {
  console.log(`Lastskript gegen ${BASE}`);
  let felder = [];
  try {
    const r = await fetch(`${BASE}health?src=poll`, { cache: "no-store" });
    felder = (await r.json()).courts || [];
  } catch (e) {
    console.error(`FEHLER: ${BASE}health nicht erreichbar — läuft die Übertragung? (${e})`);
    process.exit(1);
  }

  const belegt = felder.filter((c) => c.match_id && c.match_id > 0);
  console.log(`${felder.length} Felder gemeldet, davon ${belegt.length} belegt.`);
  if (belegt.length === 0 && !TROCKEN) {
    console.error(
      "FEHLER: Kein belegtes Feld. Ein Stand ohne passende Match-ID wird vom Server\n" +
        "verworfen (`process_result`/`handle_score`) — es gäbe weder Schreibvorgang\n" +
        "noch Nudge, und die Messung wäre wertlos. Erst Felder belegen, dann messen.",
    );
    process.exit(2);
  }

  const tabletZahl = Math.min(TABLETS, belegt.length);
  if (tabletZahl < TABLETS) {
    console.log(`Hinweis: nur ${tabletZahl} statt ${TABLETS} Tablets — so viele Felder sind belegt.`);
  }
  for (let i = 0; i < tabletZahl; i++) starteTablet(belegt[i], i);
  for (let i = 0; i < UEBERSICHTEN; i++) starteUebersicht(i);
  for (let i = 0; i < Math.min(COURT_MONITORE, felder.length); i++) {
    starteCourtMonitor(felder[i].court_id);
  }
  console.log(
    `${tabletZahl} Tablets (${PUNKTE_PRO_MIN} Punkte/min je Tablet), ` +
      `${UEBERSICHTEN} Übersichten, ${Math.min(COURT_MONITORE, felder.length)} Court-Monitore, ` +
      `${DAUER_S} s${TROCKEN ? " — TROCKEN, es wird nichts gesendet" : ""}.`,
  );

  const begonnen = Date.now();
  let letzteZwischenzeit = begonnen;
  let letzteAbrufe = 0;
  while (Date.now() - begonnen < DAUER_S * 1000) {
    await schlaf(1000);
    if (Date.now() - letzteZwischenzeit >= 10000) {
      const s = (Date.now() - letzteZwischenzeit) / 1000;
      console.log(
        `  … ${((stat.healthAbrufe - letzteAbrufe) / s).toFixed(1)} /health-Abrufe/s, ` +
          `${stat.punkte} Punkte, ${stat.nudges} Nudges, ${stat.latenzen.length} Latenzmesswerte`,
      );
      letzteZwischenzeit = Date.now();
      letzteAbrufe = stat.healthAbrufe;
    }
  }

  const s = (Date.now() - begonnen) / 1000;
  const mb = (bytes) => (bytes / 1048576 / s).toFixed(2);
  console.log("\n── Client-Sicht ─────────────────────────────────────────────");
  console.log(`Dauer                    ${s.toFixed(0)} s`);
  console.log(`Gesendete Punkte         ${stat.punkte} (${(stat.punkte / s).toFixed(2)}/s)`);
  console.log(`Empfangene Nudges        ${stat.nudges} (${(stat.nudges / s).toFixed(1)}/s)`);
  console.log(
    `/health                  ${stat.healthAbrufe} Abrufe (${(stat.healthAbrufe / s).toFixed(1)}/s), ` +
      `${mb(stat.healthBytes)} MB/s`,
  );
  console.log(
    `/court/{id}/state        ${stat.courtAbrufe} Abrufe (${(stat.courtAbrufe / s).toFixed(1)}/s), ` +
      `${mb(stat.courtBytes)} MB/s`,
  );
  console.log(`Fehlgeschlagene Abrufe   ${stat.healthFehler}`);
  console.log(
    `Latenz Punkt → Anzeige   p50 ${perzentil(stat.latenzen, 50)} ms, ` +
      `p95 ${perzentil(stat.latenzen, 95)} ms (${stat.latenzen.length} Messwerte)`,
  );

  try {
    const r = await fetch(`${BASE}debug/perf`, { cache: "no-store" });
    if (r.ok) {
      const p = await r.json();
      console.log("\n── Server-Sicht (/debug/perf, seit Programmstart) ───────────");
      for (const [k, v] of Object.entries(p)) console.log(`${k.padEnd(24)} ${v}`);
    } else {
      console.log("\n(/debug/perf nicht verfügbar — im Cloud-Modus gibt es die Route nicht.)");
    }
  } catch {
    console.log("\n(/debug/perf nicht erreichbar — im Cloud-Modus gibt es die Route nicht.)");
  }
  process.exit(0);
}

main();
