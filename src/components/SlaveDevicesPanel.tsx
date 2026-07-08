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
 * Link** für den TV. Beides zeigt auf den Namespace des Masters, die Ergebnisse
 * fließen darüber zurück ins Master-BTP.
 *
 * Sichtbar nur, wenn dieser PC als Cloud-Slave läuft (`slave_devices` liefert
 * dann eine `relay_base`); sonst rendert die Komponente nichts.
 */
export function SlaveDevicesPanel() {
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

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-violet-300 bg-violet-50 p-5 shadow-sm">
      <div className="flex items-center gap-2 font-medium text-violet-900">
        <MonitorSmartphone size={18} className="shrink-0" />
        Geräte dieser Halle anschließen
        {info.hall && (
          <span className="rounded bg-violet-200 px-1.5 py-0.5 text-xs font-semibold text-violet-900">
            {info.hall}
          </span>
        )}
      </div>
      <p className="text-sm text-violet-800">
        Tablets und TVs dieser Halle verbinden sich <strong>direkt über die
        Cloud</strong> mit dem Master. Am Feld den <strong>Tablet-QR</strong>{" "}
        scannen; am TV den <strong>Monitor-Link</strong> im Browser öffnen.
      </p>

      {!info.hall && (
        <p className="rounded-lg bg-amber-50 px-3 py-2 text-sm text-amber-800">
          Noch <strong>keine Halle</strong> gewählt — es werden{" "}
          <strong>alle</strong> Felder gezeigt. Unter „Sprachansagen" die Halle
          dieser fernen Halle wählen, dann bleibt nur sie übrig.
        </p>
      )}
      {info.courts.length === 0 ? (
        <p className="rounded-lg bg-white/70 px-3 py-2 text-sm text-violet-700">
          Warte auf den Master … Sobald der Master läuft (Cloud aktiv, BTP
          geladen), erscheinen hier die Felder dieser Halle.
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
