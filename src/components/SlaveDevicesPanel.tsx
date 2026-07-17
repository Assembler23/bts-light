import { useEffect, useState } from "react";
import { MonitorSmartphone, Copy, ExternalLink } from "lucide-react";
import { slaveDevices } from "../api";
import type { SlaveDeviceInfo } from "../types";

const POLL_MS = 5000;

/**
 * Geräte-Anschluss der fernen Halle (Slave, Weg A / Direkt-Cloud).
 *
 * In der fernen Halle hängen Tablets und TVs **direkt** am Cloud-Relay des
 * Masters — nicht am Slave-PC. Der Slave-PC sagt nur an. Diese Ansicht liefert
 * der Crew vor Ort die Adressen ihrer Felder, ohne auf den Master-Bildschirm
 * schauen zu müssen: je Feld ein scannbarer **Tablet-QR** und der **Monitor-
 * Link** für den TV.
 *
 * **Hallen-Auswahl:** Der Cloud-Slave hat kein BTP und kann die Hallennamen
 * nicht aus einem lokalen Snapshot ziehen. Er bekommt sie deshalb über die
 * Relay-Feldliste (`all_halls`) und lässt die Halle hier wählen — dieselbe
 * `announce_hall`, die auch die Ansage steuert. Ohne gewählte Halle würde der
 * Slave alle Hallen ansagen (auch die des Masters) und alle Felder zeigen;
 * darum wird die Auswahl hier erzwungen.
 *
 * Sichtbar nur, wenn dieser PC als Cloud-Slave läuft (`slave_devices` liefert
 * dann eine `relay_base`); sonst rendert die Komponente nichts.
 */
export function SlaveDevicesPanel({
  announceHall,
  onPickHall,
}: {
  announceHall: string;
  onPickHall: (hall: string) => void;
}) {
  const [info, setInfo] = useState<SlaveDeviceInfo | null>(null);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      slaveDevices()
        .then((d) => {
          if (alive) setInfo(d);
        })
        .catch(() => {});
    };
    tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  // Kein Cloud-Slave (keine Relay-Basis) → nichts anzeigen.
  if (!info || !info.relay_base) return null;

  const base = info.relay_base;
  const hallChosen = announceHall !== "";

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-violet-300 bg-violet-50 p-5 shadow-sm">
      <div className="flex items-center gap-2 font-medium text-violet-900">
        <MonitorSmartphone size={18} className="shrink-0" />
        Geräte dieser Halle anschließen
        {announceHall && (
          <span className="rounded bg-violet-200 px-1.5 py-0.5 text-xs font-semibold text-violet-900">
            {announceHall}
          </span>
        )}
      </div>
      <p className="text-sm text-violet-800">
        Tablets und TVs dieser Halle verbinden sich <strong>direkt über die
        Cloud</strong> mit dem Master. Am Feld den <strong>Tablet-QR</strong>{" "}
        scannen; am TV den <strong>Monitor-Link</strong> im Browser öffnen.
      </p>

      {/* Hallen-Auswahl (aus der Relay-Feldliste; steuert Ansage + Filter). */}
      {info.all_halls.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg bg-white/70 px-3 py-2">
          <label className="text-sm font-medium text-violet-900">
            Diese ferne Halle ist:
          </label>
          <select
            value={announceHall}
            onChange={(e) => onPickHall(e.currentTarget.value)}
            className="rounded-lg border border-violet-300 bg-white px-2.5 py-1.5 text-sm
                       text-slate-800 focus:border-violet-500 focus:outline-none"
          >
            <option value="">— bitte wählen —</option>
            {info.all_halls.map((h) => (
              <option key={h} value={h}>
                {h}
              </option>
            ))}
          </select>
          <span className="text-xs text-violet-700">
            steuert zugleich die Ansage (nur diese Halle)
          </span>
        </div>
      )}

      {!hallChosen ? (
        <p className="rounded-lg bg-amber-50 px-3 py-2 text-sm text-amber-800">
          {info.all_halls.length > 0 ? (
            <>
              Bitte oben die <strong>Halle</strong> dieser fernen Halle wählen.
              Sonst werden <strong>alle</strong> Hallen angesagt (auch die des
              Masters) und die Feld-Codes gehören zur falschen Halle.
            </>
          ) : (
            <>
              Warte auf den Master … Sobald der Master läuft (Cloud aktiv, BTP
              geladen), erscheinen hier die Hallen und Felder.
            </>
          )}
        </p>
      ) : info.courts.length === 0 ? (
        <p className="rounded-lg bg-white/70 px-3 py-2 text-sm text-violet-700">
          Keine Felder für „{announceHall}" gefunden. Stimmt der Hallenname mit
          BTP überein? Sonst oben eine andere Halle wählen.
        </p>
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {info.courts.map((c) => (
            <CourtCard key={c.id} base={base} id={c.id} label={c.label} />
          ))}
        </div>
      )}
    </section>
  );
}

/** Eine Karte je Feld: Tablet-QR + Monitor-Link. */
function CourtCard({
  base,
  id,
  label,
}: {
  base: string;
  id: number;
  label: string;
}) {
  const monitorUrl = `${base}/court/${id}/display`;
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-violet-200 bg-white p-3">
      <div className="text-sm font-semibold text-violet-900">{label}</div>
      {/* Tablet: scannbarer QR (zeigt auf die Tablet-Seite des Felds). */}
      <img
        src={`${base}/qr/${id}`}
        alt={`Tablet-QR ${label}`}
        className="h-32 w-32 self-center"
      />
      <div className="text-center text-xs text-violet-700">
        Tablet: QR scannen
      </div>
      {/* Monitor (TV): Link zum Öffnen/Kopieren. */}
      <div className="mt-1 flex items-center gap-1.5 border-t border-violet-100 pt-2">
        <a
          href={monitorUrl}
          target="_blank"
          rel="noreferrer"
          className="flex items-center gap-1 text-xs font-medium text-violet-700 hover:text-violet-900"
        >
          <ExternalLink size={13} /> Monitor (TV)
        </a>
        <button
          type="button"
          title="Monitor-Link kopieren"
          onClick={() => void navigator.clipboard?.writeText(monitorUrl)}
          className="ml-auto flex items-center gap-1 rounded bg-violet-100 px-1.5 py-0.5 text-xs
                     text-violet-800 transition-colors hover:bg-violet-200"
        >
          <Copy size={12} /> Link
        </button>
      </div>
    </div>
  );
}
