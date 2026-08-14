// Generischer Baustein für Listen, deren Reihenfolge sich per Zug ändern
// lässt (aktuell: Schiedsrichter-Rotation in OfficialsPanel.tsx; geplant:
// die Spielliste). Bewusst ohne jeden Bezug zur Schiedsrichter-Fachlichkeit
// — wer eine weitere Liste ziehbar machen will, ruft `useDragReorder` mit
// den eigenen Einträgen auf und verdrahtet `dragHandleProps`/`registerRow`
// an eine eigene Zeilen-Komponente.
//
// Muster geteilt mit `assets/tl.html::enableReorderDrag` (dasselbe Problem,
// andere Umgebung: dort direktes DOM-Umsortieren, hier React-State) —
// derselbe Kerngedanke: Die gezogene Zeile wandert schon WÄHREND der Geste
// live an ihre neue Position (Vergleich der Zeiger-Y-Position gegen die
// Mittelpunkte der übrigen Zeilen), nicht erst nach dem Loslassen. Das gibt
// sofortige, natürliche Rückmeldung ohne separaten Geist/Platzhalter.
import { useEffect, useRef, useState } from "react";

export interface DragReorderResult<T> {
  /** Aktuelle Anzeige-Reihenfolge — während eines Zugs schon live verschoben. */
  order: T[];
  /** An jede Zeile binden (z. B. `ref={(el) => registerRow(getId(item), el)}`),
   *  damit die Geste die tatsächliche Bildschirmposition kennt. */
  registerRow: (id: number, el: HTMLElement | null) => void;
  /** An das Zieh-Griff-Element der Zeile binden (Spread: `{...dragHandleProps(id)}`).
   *  `onKeyDown` deckt Pfeil-hoch/-runter ab — ohne die frühere
   *  Pfeil-Bedienung wäre sonst kein Tastatur-/Screenreader-Weg mehr da,
   *  die Reihenfolge zu ändern (Code-Review-Fund 14.08.2026). */
  dragHandleProps: (id: number) => {
    onPointerDown: (e: React.PointerEvent) => void;
    onKeyDown: (e: React.KeyboardEvent) => void;
  };
  /** ID der gerade gezogenen Zeile, sonst `null` — für Hervorhebung. */
  draggingId: number | null;
}

/**
 * `items`: aktuelle Daten vom Server (in Rotationsreihenfolge).
 * `getId`: liefert die stabile ID eines Eintrags.
 * `onReorder`: aufgerufen, wenn sich die Reihenfolge beim Loslassen
 * tatsächlich geändert hat; `beforeId=null` heißt „ans Ende".
 */
export function useDragReorder<T>(
  items: T[],
  getId: (item: T) => number,
  onReorder: (id: number, beforeId: number | null) => void,
): DragReorderResult<T> {
  const [order, setOrder] = useState(items);
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const draggingRef = useRef<number | null>(null);
  const rowRefs = useRef(new Map<number, HTMLElement>());

  // Neue Daten vom Server übernehmen die Anzeige — außer gerade wird
  // gezogen, dann gewinnt die laufende Geste (sonst risse ein Poll mitten
  // im Zug die Zeile unter dem Finger weg).
  useEffect(() => {
    if (draggingRef.current === null) setOrder(items);
  }, [items]);

  function registerRow(id: number, el: HTMLElement | null) {
    if (el) rowRefs.current.set(id, el);
    else rowRefs.current.delete(id);
  }

  function dragHandleProps(id: number) {
    return {
      onPointerDown: (down: React.PointerEvent) => {
        if (down.pointerType !== "touch" && down.button !== 0) return;
        // Ein zweiter Finger (z. B. der Handballen) darf keinen eigenen,
        // parallelen Zug starten — dessen Loslassen würde sonst
        // `draggingRef.current` unbedingt auf `null` setzen und den
        // Poll-Schutz oben mitten im ERSTEN Zug wieder scharfschalten
        // (Code-Review-Fund 14.08.2026, Muster `reorderDragPending` in
        // `assets/tl.html::enableReorderDrag`).
        if (draggingRef.current !== null) return;
        down.preventDefault();
        const pointerId = down.pointerId;
        const target = down.currentTarget as HTMLElement;
        draggingRef.current = id;
        setDraggingId(id);
        const before = order;
        try {
          target.setPointerCapture(pointerId);
        } catch {
          /* Zeiger schon weg */
        }

        const move = (ev: PointerEvent) => {
          if (ev.pointerId !== pointerId) return;
          setOrder((cur) => {
            const draggedIdx = cur.findIndex((it) => getId(it) === id);
            if (draggedIdx === -1) return cur;
            const dragged = cur[draggedIdx];
            const rest = cur.filter((it) => getId(it) !== id);
            let insertAt = rest.length;
            for (let i = 0; i < rest.length; i++) {
              const el = rowRefs.current.get(getId(rest[i]));
              if (!el) continue;
              const r = el.getBoundingClientRect();
              if (ev.clientY < r.top + r.height / 2) {
                insertAt = i;
                break;
              }
            }
            rest.splice(insertAt, 0, dragged);
            return rest;
          });
        };

        const finish = (cancelled: boolean) => {
          window.removeEventListener("pointermove", move);
          window.removeEventListener("pointerup", up);
          window.removeEventListener("pointercancel", cancel);
          draggingRef.current = null;
          setDraggingId(null);
          try {
            target.releasePointerCapture(pointerId);
          } catch {
            /* Zeiger schon weg */
          }
          if (cancelled) {
            setOrder(before);
            return;
          }
          setOrder((cur) => {
            const changed = cur.some((it, i) => getId(it) !== getId(before[i]));
            if (changed) {
              const i = cur.findIndex((it) => getId(it) === id);
              const beforeId = i + 1 < cur.length ? getId(cur[i + 1]) : null;
              onReorder(id, beforeId);
            }
            return cur;
          });
        };
        const up = (ev: PointerEvent) => {
          if (ev.pointerId === pointerId) finish(false);
        };
        const cancel = (ev: PointerEvent) => {
          if (ev.pointerId === pointerId) finish(true);
        };
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", up);
        window.addEventListener("pointercancel", cancel);
      },
      onKeyDown: (ev: React.KeyboardEvent) => {
        if (ev.key !== "ArrowUp" && ev.key !== "ArrowDown") return;
        ev.preventDefault();
        const i = order.findIndex((it) => getId(it) === id);
        if (i === -1) return;
        if (ev.key === "ArrowUp") {
          if (i === 0) return;
          onReorder(id, getId(order[i - 1]));
        } else {
          if (i >= order.length - 1) return;
          const beforeId = i + 2 < order.length ? getId(order[i + 2]) : null;
          onReorder(id, beforeId);
        }
      },
    };
  }

  return { order, registerRow, dragHandleProps, draggingId };
}
