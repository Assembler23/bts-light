import { useEffect, useState } from "react";
import {
  ArrowLeft,
  Battery,
  BatteryCharging,
  BatteryWarning,
  Check,
  Cloud,
  Copy,
  Info,
  Wifi,
} from "lucide-react";
import { tabletOverview } from "../api";
import type { CourtOverview, TabletInfo } from "../types";

interface Props {
  onBack: () => void;
}

/**
 * Tablet-Spielzettel-Seite: oben die Adressen/QR-Codes zum Einrichten der
 * Tablets, darunter die Live-Felder-Übersicht für die Turnierleitung.
 * Beide Bereiche sind Raster – sie skalieren bis zu 20–30 Spielfeldern.
 * Pollt den Tablet-Server alle 2 s.
 */
export function TabletPanel({ onBack }: Props) {
  const [info, setInfo] = useState<TabletInfo | null>(null);
  const [zoomCourt, setZoomCourt] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const tick = () => {
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

  const host = info?.server_host ?? "";
  const isCloud = (info?.mode ?? "lan") === "cloud";
  const relayBase = info?.relay_base ?? "";
  const courts = info?.courts ?? [];
  const courtUrl = (court: string) =>
    isCloud
      ? `${relayBase}/court/${encodeURIComponent(court)}`
      : `http://${host}/court/${encodeURIComponent(court)}`;
  const qrUrl = (court: string) =>
    isCloud
      ? `${relayBase}/qr/${encodeURIComponent(court)}`
      : `http://${host}/qr/${encodeURIComponent(court)}`;

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
          <h1 className="text-2xl font-semibold leading-tight">
            Tablet-Spielzettel
          </h1>
          <p className="text-sm text-slate-500">
            {courts.length > 0
              ? `${courts.length} Spielfelder`
              : "Einrichtung & Felder-Übersicht"}
          </p>
        </div>
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1
                      text-xs font-medium ${
                        isCloud
                          ? "bg-sky-100 text-sky-700"
                          : "bg-slate-200 text-slate-600"
                      }`}
          title={
            isCloud
              ? "Cloud-Modus: Tablets verbinden über badhub.de"
              : "LAN-Modus: Tablets verbinden im lokalen Netz"
          }
        >
          {isCloud ? <Cloud size={14} /> : <Wifi size={14} />}
          {isCloud ? "Cloud" : "LAN"}
        </span>
      </header>

      {/* Firewall-Hinweis – nur im LAN-Modus relevant. */}
      {!isCloud && (
        <div className="flex gap-2.5 rounded-xl border border-amber-200 bg-amber-50 p-3.5 text-sm text-amber-900">
          <Info size={18} className="mt-0.5 shrink-0 text-amber-500" />
          <p>
            <span className="font-medium">
              Bekommen die Tablets keine Verbindung?
            </span>{" "}
            Auf IT-verwalteten Turnier-PCs blockiert die Firewall oft den
            Zugriff im lokalen Netz. Dann in den Einstellungen die
            Tablet-Verbindung auf{" "}
            <span className="font-medium">„Über badhub.de (Cloud)"</span>{" "}
            umstellen – das funktioniert auch hinter gesperrten Firewalls.
          </p>
        </div>
      )}

      {courts.length === 0 ? (
        <p className="rounded-xl border border-slate-200 bg-white p-5 text-sm text-slate-500 shadow-sm">
          Noch keine Spielfelder geladen. Starte den Liveticker (BTP muss
          verbunden sein) – danach erscheinen hier die Tablet-Adressen für
          alle Felder. Die Zahl der Tablets ist nicht begrenzt.
        </p>
      ) : (
        <>
          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-slate-700">
              Tablet-Adressen
            </h2>
            <p className="text-xs text-slate-500">
              Am Spielfeld die Adresse im Browser öffnen oder den QR-Code
              scannen (auf den QR tippen zeigt ihn groß).{" "}
              {isCloud
                ? "Tablet und PC brauchen je eine Internet-Verbindung – kein gemeinsames WLAN nötig."
                : "Tablet und dieser PC müssen im selben WLAN sein."}
            </p>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              {courts.map((c) => (
                <div
                  key={c.court}
                  className="flex items-center gap-3 rounded-lg border border-slate-200 bg-white p-2 shadow-sm"
                >
                  <button
                    onClick={() => setZoomCourt(c.court)}
                    title="QR-Code groß anzeigen"
                    className="shrink-0 rounded bg-white"
                  >
                    <img
                      src={qrUrl(c.court)}
                      alt=""
                      width={64}
                      height={64}
                      className="block"
                    />
                  </button>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">
                      {c.court}
                    </div>
                    <div className="truncate text-xs text-slate-500">
                      {courtUrl(c.court)}
                    </div>
                  </div>
                  <CopyUrlButton url={courtUrl(c.court)} />
                </div>
              ))}
            </div>
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-slate-700">
              Felder-Übersicht
            </h2>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              {courts.map((c) => (
                <CourtCard key={c.court} court={c} />
              ))}
            </div>
          </section>
        </>
      )}

      {zoomCourt !== null && (
        <div
          onClick={() => setZoomCourt(null)}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-6"
        >
          <div className="flex flex-col items-center rounded-xl bg-white p-6 text-center">
            <img
              src={qrUrl(zoomCourt)}
              alt=""
              className="bg-white"
              style={{ width: "min(72vw, 72vh)", height: "min(72vw, 72vh)" }}
            />
            <div className="mt-3 text-lg font-semibold">{zoomCourt}</div>
            <div className="mt-1 max-w-[20rem] break-all text-sm text-slate-500">
              {courtUrl(zoomCourt)}
            </div>
            <div className="mt-3 text-xs text-slate-400">
              Zum Schließen tippen
            </div>
          </div>
        </div>
      )}
    </main>
  );
}

function CourtCard({ court }: { court: CourtOverview }) {
  const team1 = court.team1.join(" / ") || "—";
  const team2 = court.team2.join(" / ") || "—";
  const hasMatch = court.match_name !== "" || court.team1.length > 0;
  const cardClass = court.injury
    ? "border-rose-400 bg-rose-50"
    : court.official_call
      ? "border-amber-400 bg-amber-50"
      : "border-slate-200 bg-white";

  return (
    <div className={`rounded-lg border p-3 shadow-sm ${cardClass}`}>
      {(court.injury || court.official_call) && (
        <div className="mb-1 flex flex-wrap gap-x-3 text-xs font-semibold">
          {court.injury && (
            <span className="text-rose-700">✚ Verletzung / Behandlung</span>
          )}
          {court.official_call && (
            <span className="text-amber-700">📣 Turnierleitung gerufen</span>
          )}
        </div>
      )}
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-medium">{court.court}</span>
        <span className="flex shrink-0 items-center gap-2 text-xs text-slate-500">
          {court.battery && <BatteryBadge battery={court.battery} />}
          <span className="flex items-center gap-1.5">
            <span
              className={`h-2 w-2 rounded-full ${
                court.tablet_connected ? "bg-emerald-500" : "bg-slate-300"
              }`}
            />
            {court.tablet_connected ? "Tablet" : "kein Tablet"}
          </span>
        </span>
      </div>
      {hasMatch ? (
        <>
          {court.match_name !== "" && (
            <div className="mt-0.5 truncate text-xs text-slate-500">
              {court.match_name}
            </div>
          )}
          <div className="mt-1 flex justify-between gap-3 text-sm">
            <span className="min-w-0 truncate">{team1}</span>
            <span className="shrink-0 font-mono font-semibold tabular-nums">
              {court.sets.map((s) => s[0]).join("  ")}
            </span>
          </div>
          <div className="flex justify-between gap-3 text-sm">
            <span className="min-w-0 truncate">{team2}</span>
            <span className="shrink-0 font-mono font-semibold tabular-nums">
              {court.sets.map((s) => s[1]).join("  ")}
            </span>
          </div>
        </>
      ) : (
        <div className="mt-1 text-sm text-slate-400">frei</div>
      )}
    </div>
  );
}

/** Kleiner Button, der eine Tablet-Adresse in die Zwischenablage kopiert. */
function CopyUrlButton({ url }: { url: string }) {
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

/** Akkustand-Anzeige eines Tablets (nur Android/Chrome liefern ihn). */
function BatteryBadge({
  battery,
}: {
  battery: { percent: number; charging: boolean };
}) {
  const { percent, charging } = battery;
  const color =
    percent < 20
      ? "text-rose-600"
      : percent < 50
        ? "text-amber-600"
        : "text-slate-500";
  const Icon = charging
    ? BatteryCharging
    : percent < 20
      ? BatteryWarning
      : Battery;
  return (
    <span
      className={`flex items-center gap-1 tabular-nums ${color}`}
      title={charging ? "Tablet-Akku (lädt)" : "Tablet-Akku"}
    >
      <Icon size={14} />
      {percent}%
    </span>
  );
}
