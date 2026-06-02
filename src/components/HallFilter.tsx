// Kleiner Hallen-Umschalter („Alle | Halle 1 | Halle 2 …") für Mehr-Hallen-
// Turniere. Erst ab 2 Hallen sichtbar; darunter rendert er nichts.
export function HallFilter({
  halls,
  value,
  onChange,
}: {
  halls: string[];
  /** Aktive Halle, oder null für „Alle". */
  value: string | null;
  onChange: (hall: string | null) => void;
}) {
  if (halls.length < 2) return null;

  const chip = (label: string, active: boolean, on: () => void) => (
    <button
      key={label}
      onClick={on}
      aria-pressed={active}
      className={`rounded-full px-3 py-1 text-sm font-medium transition-colors ${
        active
          ? "bg-slate-800 text-white"
          : "bg-slate-100 text-slate-600 hover:bg-slate-200"
      }`}
    >
      {label}
    </button>
  );

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="mr-1 text-xs font-medium text-slate-500">Halle:</span>
      {chip("Alle", value === null, () => onChange(null))}
      {halls.map((h) => chip(h, value === h, () => onChange(h)))}
    </div>
  );
}
