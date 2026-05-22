import { useEffect, useState } from "react";
import {
  ArrowLeft,
  Check,
  Cloud,
  Copy,
  Info,
  RefreshCw,
  Search,
  Tv,
  Wifi,
} from "lucide-react";
import { assignMonitor, monitorCommand, monitorDevices, tabletOverview } from "../api";
import type { CourtOverview, MonitorDeviceInfo, TabletInfo } from "../types";

interface Props {
  onBack: () => void;
}

/**
 * Court-Monitore-Seite: oben die eine Einrichtungs-Adresse für alle
 * Raspberry Pis, darunter die Geräteliste mit Online-Status, Feld-
 * Zuweisung und Fernbefehlen. Pollt im 2-s-Takt.
 */
export function CourtMonitorPanel({ onBack }: Props) {
  const [devices, setDevices] = useState<MonitorDeviceInfo[]>([]);
  const [info, setInfo] = useState<TabletInfo | null>(null);

  useEffect(() => {
    let active = true;
    const tick = () => {
      monitorDevices()
        .then((d) => {
          if (active) setDevices(d);
        })
        .catch(() => {});
      tabletOverview()
        .then((i) => {
          if (active) setInfo(i);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  const isCloud = (info?.mode ?? "lan") === "cloud";
  // Im LAN-Modus der feste mDNS-Name (muss zu MDNS_HOST + TABLET_PORT im
  // Rust-Kern passen) – so braucht es keine feste IP. Die IP-Adresse dient
  // nur als Rückfall, falls der Name im Netz nicht aufgelöst wird.
  const monitorUrl = isCloud
    ? `${info?.relay_base ?? ""}/monitor`
    : "http://bts-light.local:8088/monitor";
  const fallbackUrl =
    !isCloud && info?.server_host ? `http://${info.server_host}/monitor` : "";
  // Felder mit Identität (CourtID) und Anzeigename – die Zuweisung nutzt
  // die CourtID, das <select> zeigt den Namen.
  const courts: CourtOverview[] = info?.courts ?? [];

  async function refresh() {
    try {
      setDevices(await monitorDevices());
    } catch {
      /* ignorieren – nächster Poll versucht es erneut */
    }
  }

  async function assign(deviceId: string, courtId: number | null) {
    try {
      await assignMonitor(deviceId, courtId);
      await refresh();
    } catch {
      /* ignorieren */
    }
  }

  return (
    <main className="mx-auto flex min-h-full max-w-4xl flex-col gap-5 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <button
          onClick={onBack}
          title="Zurück zum Dashboard"
          className="inline-flex items-center gap-1.5 rounded-lg bg-slate-100 px-3 py-1.5
                     text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
        >
          <ArrowLeft size={16} />
          Zurück
        </button>
        <div className="flex-1">
          <h1 className="text-2xl font-semibold leading-tight">Court-Monitore</h1>
          <p className="text-sm text-slate-500">
            {devices.length > 0
              ? `${devices.length} ${devices.length === 1 ? "Gerät" : "Geräte"}`
              : "TV-Anzeigen am Spielfeld"}
          </p>
        </div>
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs
                      font-medium ${
                        isCloud
                          ? "bg-sky-100 text-sky-700"
                          : "bg-slate-200 text-slate-600"
                      }`}
        >
          {isCloud ? <Cloud size={14} /> : <Wifi size={14} />}
          {isCloud ? "Cloud" : "LAN"}
        </span>
      </header>

      {/* Einrichtungs-Adresse für alle Pis */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Einrichtung am Raspberry Pi
        </h2>
        <p className="text-xs text-slate-500">
          Alle Monitore bekommen <span className="font-medium">dieselbe</span>{" "}
          Adresse – im Chromium-Kiosk öffnen. Das Gerät zeigt dann einen Code;
          ordne es unten einem Feld zu.
        </p>
        <div className="flex items-center gap-3 rounded-lg border border-slate-200 bg-white p-2.5 shadow-sm">
          <Tv size={18} className="shrink-0 text-slate-400" />
          <code className="min-w-0 flex-1 truncate text-sm">{monitorUrl}</code>
          <CopyButton url={monitorUrl} />
        </div>
        {fallbackUrl && (
          <p className="text-xs text-slate-400">
            Falls der Name <code>bts-light.local</code> im Netz nicht
            gefunden wird, alternativ:{" "}
            <code className="text-slate-500">{fallbackUrl}</code>
          </p>
        )}
      </section>

      {/* Geräteliste */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">Geräte</h2>
        {devices.length === 0 ? (
          <div className="flex gap-2.5 rounded-xl border border-slate-200 bg-white p-4 text-sm text-slate-500 shadow-sm">
            <Info size={18} className="mt-0.5 shrink-0 text-slate-400" />
            <p>
              Noch keine Monitore gemeldet. Richte einen Raspberry Pi mit der
              Adresse oben ein – sobald er die Seite öffnet, erscheint er hier.
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {devices.map((d) => (
              <DeviceRow
                key={d.id}
                device={d}
                courts={courts}
                onAssign={(courtId) => void assign(d.id, courtId)}
                onIdentify={() => void monitorCommand(d.id, "identify")}
                onReload={() => void monitorCommand(d.id, "reload")}
              />
            ))}
          </div>
        )}
      </section>
    </main>
  );
}

function DeviceRow({
  device,
  courts,
  onAssign,
  onIdentify,
  onReload,
}: {
  device: MonitorDeviceInfo;
  courts: CourtOverview[];
  onAssign: (courtId: number | null) => void;
  onIdentify: () => void;
  onReload: () => void;
}) {
  // Optionen des <select>: value = CourtID (Identität), Text = Feldname.
  // Falls einem Gerät ein Feld zugewiesen ist, das nicht (mehr) in der
  // Court-Liste steht, trotzdem als Option führen.
  const options: { id: number; label: string; location: string }[] =
    courts.map((c) => ({
      id: c.court_id,
      label: c.court,
      location: c.location,
    }));
  if (
    device.courtId !== null &&
    !options.some((o) => o.id === device.courtId)
  ) {
    options.unshift({
      id: device.courtId,
      label: device.court ?? `Feld ${device.courtId}`,
      location: "",
    });
  }
  // Mehr-Hallen-Turnier (≥2 distinkte, nicht-leere Hallennamen): die
  // <option>s pro Halle in <optgroup> bündeln. Sonst flache Liste.
  const hallNames = [
    ...new Set(options.map((o) => o.location).filter((l) => l !== "")),
  ];
  const grouped = hallNames.length >= 2;

  return (
    <div className="flex flex-wrap items-center gap-3 rounded-lg border border-slate-200 bg-white p-3 shadow-sm">
      <span
        className={`h-2.5 w-2.5 shrink-0 rounded-full ${
          device.online ? "bg-emerald-500" : "bg-slate-300"
        }`}
        title={device.online ? "Online" : "Offline"}
      />
      <span className="font-mono text-base font-bold tracking-wider">
        {device.code}
      </span>
      <span className="text-xs text-slate-400">
        {device.online ? "online" : "offline"}
      </span>

      <select
        value={device.courtId ?? ""}
        onChange={(e) => {
          const v = e.currentTarget.value;
          onAssign(v === "" ? null : Number(v));
        }}
        className="ml-auto rounded-lg border border-slate-300 bg-white px-2.5 py-1.5 text-sm
                   focus:border-slate-500 focus:outline-none"
      >
        <option value="">— kein Feld —</option>
        {grouped ? (
          <>
            {hallNames.map((hall) => (
              <optgroup key={hall} label={hall}>
                {options
                  .filter((o) => o.location === hall)
                  .map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.label}
                    </option>
                  ))}
              </optgroup>
            ))}
            {/* Felder ohne auflösbare Halle (z. B. nach Turnierwechsel)
                bleiben ohne <optgroup> erhalten, damit keine Zuweisung
                aus der Liste verschwindet. */}
            {options
              .filter((o) => o.location === "")
              .map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
          </>
        ) : (
          options.map((o) => (
            <option key={o.id} value={o.id}>
              {o.label}
            </option>
          ))
        )}
      </select>

      <button
        onClick={onIdentify}
        title="Code + Feld groß am Monitor einblenden"
        className="inline-flex items-center gap-1.5 rounded-lg bg-slate-100 px-2.5 py-1.5
                   text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
      >
        <Search size={15} />
        Identifizieren
      </button>
      <button
        onClick={onReload}
        title="Monitor-Seite neu laden"
        className="inline-flex items-center gap-1.5 rounded-lg bg-slate-100 px-2.5 py-1.5
                   text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
      >
        <RefreshCw size={15} />
        Neu laden
      </button>
    </div>
  );
}

/** Kleiner Button, der die Monitor-Adresse in die Zwischenablage kopiert. */
function CopyButton({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* Zwischenablage nicht verfügbar – ignorieren */
    }
  }
  return (
    <button
      onClick={copy}
      title="Adresse kopieren"
      className="shrink-0 rounded-md p-1.5 text-slate-400 transition-colors
                 hover:bg-slate-100 hover:text-slate-700"
    >
      {copied ? (
        <Check size={16} className="text-emerald-600" />
      ) : (
        <Copy size={16} />
      )}
    </button>
  );
}
