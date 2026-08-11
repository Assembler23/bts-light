import type { MatchTimeline, TimelineSet } from "../types";

/**
 * Punktverlauf-Diagramm (Spec punktverlauf-graph): je Satz zwei Linien,
 * x = Ballwechsel, y = Punkte.
 *
 * JSX-Fassung derselben Geometrie wie `timelineSetSvg` in `tablet.html`
 * (kanonische Fassung dort; `tl.html` trägt die Inline-Kopie). Handgerollt
 * statt Chart-Bibliothek: zwei Polylines und ein Gitter rechtfertigen
 * keine neue Dependency, und die drei Oberflächen bleiben konsistent.
 */

/** Endstand eines Verlaufs-Satzes aus Startstand + Punktfolge. */
export function timelineSetFinal(set: TimelineSet): { a: number; b: number } {
  let a = set.startA || 0;
  let b = set.startB || 0;
  for (const c of set.points || "") {
    if (c === "A") a += 1;
    else if (c === "B") b += 1;
  }
  return { a, b };
}

const COLOR_A = "#3b82f6";
const COLOR_B = "#ef4444";

function SetChart({ set }: { set: TimelineSet }) {
  let a = set.startA || 0;
  let b = set.startB || 0;
  const curveA: [number, number][] = [[0, a]];
  const curveB: [number, number][] = [[0, b]];
  const seq = set.points || "";
  for (let i = 0; i < seq.length; i += 1) {
    if (seq[i] === "A") a += 1;
    else if (seq[i] === "B") b += 1;
    curveA.push([i + 1, a]);
    curveB.push([i + 1, b]);
  }
  const n = seq.length;
  const maxY = Math.max(a, b, 5);
  const W = 420;
  const H = 190;
  const L = 30;
  const R = 12;
  const T = 10;
  const B = 22;
  const sx = (x: number) => L + (W - L - R) * (n ? x / n : 0);
  const sy = (y: number) => H - B - (H - B - T) * (y / maxY);
  const line = (pts: [number, number][]) =>
    pts.map((p) => `${sx(p[0]).toFixed(1)},${sy(p[1]).toFixed(1)}`).join(" ");
  const gridYs: number[] = [];
  for (let y = 0; y <= maxY; y += 5) gridYs.push(y);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} role="img" className="block h-auto w-full text-slate-700">
      {gridYs.map((y) => (
        <g key={y}>
          <line
            x1={L}
            y1={sy(y)}
            x2={W - R}
            y2={sy(y)}
            stroke="currentColor"
            strokeOpacity=".14"
            strokeWidth="1"
          />
          <text
            x={L - 4}
            y={sy(y) + 3}
            textAnchor="end"
            fontSize="9"
            fill="currentColor"
            fillOpacity=".55"
          >
            {y}
          </text>
        </g>
      ))}
      <text
        x={W - R}
        y={H - 6}
        textAnchor="end"
        fontSize="9"
        fill="currentColor"
        fillOpacity=".55"
      >
        {n} Ballwechsel
      </text>
      <polyline points={line(curveA)} fill="none" stroke={COLOR_A} strokeWidth="2" />
      <polyline points={line(curveB)} fill="none" stroke={COLOR_B} strokeWidth="2" />
      {n === 0 && (
        <>
          <circle cx={sx(0)} cy={sy(set.startA || 0)} r="3.5" fill={COLOR_A} />
          <circle cx={sx(0)} cy={sy(set.startB || 0)} r="3.5" fill={COLOR_B} />
        </>
      )}
    </svg>
  );
}

/**
 * Der komplette Verlauf eines Matches: Legende, je Satz Überschrift +
 * Diagramm, dazu die Kennzeichnungen (Zwischenstand, Aufgabe, Abweichung).
 *
 * `finishedSets` (Team1/Team2-Paare aus der Beendet-Tabelle) aktiviert den
 * Abweichungs-Hinweis: BTP bleibt die Wahrheit (R2), der Graph sagt dann
 * ehrlich, dass er vom gewerteten Ergebnis abweicht (AK-8).
 */
export function TimelineChart({
  timeline,
  nameA,
  nameB,
  finishedSets,
}: {
  timeline: MatchTimeline;
  nameA: string;
  nameB: string;
  finishedSets?: [number, number][];
}) {
  const sets = timeline.sets ?? [];
  const abweichung =
    finishedSets !== undefined &&
    timeline.finished &&
    !timeline.retired &&
    !(
      finishedSets.length === sets.length &&
      finishedSets.every(([fa, fb], i) => {
        const f = timelineSetFinal(sets[i]);
        return f.a === fa && f.b === fb;
      })
    );
  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap gap-4 text-xs">
        <span style={{ color: COLOR_A }}>■ {nameA}</span>
        <span style={{ color: COLOR_B }}>■ {nameB}</span>
      </div>
      {sets.length === 0 && (
        <p className="text-xs text-slate-500">Noch kein Ballwechsel aufgezeichnet.</p>
      )}
      {sets.map((s, i) => {
        const f = timelineSetFinal(s);
        const zwischenstand = s.startA || s.startB ? " · ab Zwischenstand aufgezeichnet" : "";
        return (
          <div key={i}>
            <div className="mt-2 text-xs font-semibold text-slate-600">
              Satz {i + 1} — {f.a}:{f.b}
              {zwischenstand}
            </div>
            <SetChart set={s} />
          </div>
        );
      })}
      {timeline.midGame && (
        <p className="text-xs text-slate-500">
          Teilverlauf: Die Zähltafel übernahm mit eingetipptem Zwischenstand —
          frühere Ballwechsel liegen nicht vor.
        </p>
      )}
      {timeline.retired && (
        <p className="text-xs text-slate-500">
          Das Spiel endete vorzeitig (Aufgabe/Disqualifikation) — der letzte
          Satz ist bewusst unvollständig.
        </p>
      )}
      {abweichung && (
        <p className="text-xs font-semibold text-amber-700">
          Der aufgezeichnete Verlauf weicht vom gewerteten Ergebnis ab
          (nachträgliche Korrektur in BTP) — es gilt das gewertete Ergebnis.
        </p>
      )}
    </div>
  );
}
