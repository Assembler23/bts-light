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
  LayoutGrid,
  Megaphone,
  QrCode,
  Tablet,
  Wifi,
} from "lucide-react";
import { tabletOverview } from "../api";
import type { AnnounceConfig, CourtOverview, TabletInfo } from "../types";
import { PreparationPanel } from "./PreparationPanel";

interface Props {
  onBack: () => void;
  /** Ansage-Einstellungen — werden an den „In Vorbereitung"-Tab
   *  durchgereicht, der je gerufenem Spiel eine Hallen-Ansage anbietet. */
  announce: AnnounceConfig;
}

/** Eine adressierbare Verbindung eines Felds: ein Verbindungsweg mit
 *  Court-URL und QR-URL. Im Doppelmodus hat ein Feld zwei davon. */
interface CourtAddress {
  /** Verbindungsweg – steuert Badge und Icon. */
  kind: "lan" | "cloud";
  /** Adresse, die am Spielfeld im Browser geöffnet wird. */
  courtUrl: string;
  /** URL des QR-Code-Bilds. */
  qrUrl: string;
}

/**
 * Tablet-Spielzettel-Seite mit drei Tabs: „Übersicht" zeigt den Live-Stand
 * aller Felder (Tablet-Verbindung, Akku) für die Turnierleitung;
 * „In Vorbereitung" ruft eingeplante Spiele in die Vorbereitung;
 * „QR-Codes" die Adressen/QR-Codes zum Einrichten der Tablets. Pollt
 * den Tablet-Server alle 2 s.
 */
export function TabletPanel({ onBack, announce }: Props) {
  const [info, setInfo] = useState<TabletInfo | null>(null);
  // Die groß angezeigte QR-Zoom-Ansicht: Feld + die angetippte Adresse
  // (im Doppelmodus hat ein Feld LAN und Cloud).
  const [zoom, setZoom] = useState<{
    court: CourtOverview;
    address: CourtAddress;
  } | null>(null);
  const [tab, setTab] = useState<"overview" | "preparation" | "qr">(
    "overview",
  );

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
  const relayBase = info?.relay_base ?? "";
  // LAN und Cloud sind unabhängig schaltbar – im Doppelmodus beide aktiv.
  // Fallback bei noch nicht geladenem `info`: LAN, wie bisher.
  const lanEnabled = info?.lan_enabled ?? true;
  const cloudEnabled = info?.cloud_enabled ?? false;
  // Reiner Cloud-Modus → der LAN-spezifische Firewall-Hinweis entfällt.
  const cloudOnly = cloudEnabled && !lanEnabled;
  const courts = info?.courts ?? [];
  // Pro Feld die Liste seiner Verbindungs-Adressen (CourtID-basiert). Im
  // Einzelmodus genau ein Eintrag – die Seite rendert dann wie zuvor.
  const courtAddresses = (courtId: number): CourtAddress[] => {
    const list: CourtAddress[] = [];
    if (lanEnabled) {
      list.push({
        kind: "lan",
        courtUrl: `http://${host}/court/${courtId}`,
        qrUrl: `http://${host}/qr/${courtId}`,
      });
    }
    if (cloudEnabled) {
      list.push({
        kind: "cloud",
        courtUrl: `${relayBase}/court/${courtId}`,
        qrUrl: `${relayBase}/qr/${courtId}`,
      });
    }
    return list;
  };
  // Im Doppelmodus zwei QR-Codes je Feld – dann eine Spalte, damit beide
  // nebeneinander Platz haben. Einzelmodus: zweispalt wie bisher.
  const bothModes = lanEnabled && cloudEnabled;

  // Hallenweise Gruppierung: je Halle eine Gruppe mit Überschrift. Felder
  // ohne Halle – Ein-Hallen-Turnier oder (im Mehr-Hallen-Fall) nicht
  // zugeordnete Felder – landen in einer Gruppe ohne Überschrift. Ein-
  // Hallen-Turniere sehen damit aus wie ein flaches Raster, unverändert.
  const courtGroups = groupByHall(courts);

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
        <ModeBadge lanEnabled={lanEnabled} cloudEnabled={cloudEnabled} />
      </header>

      {courts.length === 0 ? (
        <p className="rounded-xl border border-slate-200 bg-white p-5 text-sm text-slate-500 shadow-sm">
          Noch keine Spielfelder geladen. Starte den Liveticker (BTP muss
          verbunden sein) – danach erscheinen hier die Tablet-Adressen für
          alle Felder. Die Zahl der Tablets ist nicht begrenzt.
        </p>
      ) : (
        <>
          {/* Tab-Leiste: Übersicht (Live-Stand) und QR-Codes (Einrichtung). */}
          <div className="flex gap-1 border-b border-slate-200">
            <button
              onClick={() => setTab("overview")}
              className={`-mb-px inline-flex items-center gap-1.5 border-b-2 px-3.5
                          py-2 text-sm font-medium transition-colors ${
                            tab === "overview"
                              ? "border-slate-800 text-slate-800"
                              : "border-transparent text-slate-500 hover:text-slate-700"
                          }`}
            >
              <LayoutGrid size={15} />
              Übersicht
            </button>
            <button
              onClick={() => setTab("preparation")}
              className={`-mb-px inline-flex items-center gap-1.5 border-b-2 px-3.5
                          py-2 text-sm font-medium transition-colors ${
                            tab === "preparation"
                              ? "border-slate-800 text-slate-800"
                              : "border-transparent text-slate-500 hover:text-slate-700"
                          }`}
            >
              <Megaphone size={15} />
              In Vorbereitung
            </button>
            <button
              onClick={() => setTab("qr")}
              className={`-mb-px inline-flex items-center gap-1.5 border-b-2 px-3.5
                          py-2 text-sm font-medium transition-colors ${
                            tab === "qr"
                              ? "border-slate-800 text-slate-800"
                              : "border-transparent text-slate-500 hover:text-slate-700"
                          }`}
            >
              <QrCode size={15} />
              QR-Codes
            </button>
          </div>

          {tab === "overview" && (
            <section className="flex flex-col gap-2">
              <p className="text-xs text-slate-500">
                Live-Stand aller Felder mit Tablet-Verbindung und Akkustand.
              </p>
              {courtGroups.map((g) => (
                <div key={g.location} className="flex flex-col gap-2">
                  <HallHeading name={g.location} />
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                    {g.courts.map((c) => (
                      <CourtCard key={c.court_id} court={c} />
                    ))}
                  </div>
                </div>
              ))}
            </section>
          )}

          {tab === "preparation" && <PreparationPanel announce={announce} />}

          {tab === "qr" && (
            <section className="flex flex-col gap-3">
              {/* Firewall-Hinweis – nur sinnvoll, solange überhaupt ein
                  LAN-Weg aktiv ist (reiner Cloud-Modus = kein Hinweis). */}
              {!cloudOnly && (
                <div className="flex gap-2.5 rounded-xl border border-amber-200 bg-amber-50 p-3.5 text-sm text-amber-900">
                  <Info size={18} className="mt-0.5 shrink-0 text-amber-500" />
                  <p>
                    <span className="font-medium">
                      Bekommen die Tablets keine Verbindung?
                    </span>{" "}
                    Auf IT-verwalteten Turnier-PCs blockiert die Firewall oft
                    den Zugriff im lokalen Netz. Dann in den Einstellungen die
                    Tablet-Verbindung auf{" "}
                    <span className="font-medium">
                      „Über badhub.de (Cloud)"
                    </span>{" "}
                    umstellen – das funktioniert auch hinter gesperrten
                    Firewalls.
                  </p>
                </div>
              )}
              <p className="text-xs text-slate-500">
                Am Spielfeld die Adresse im Browser öffnen oder den QR-Code
                scannen (auf den QR tippen zeigt ihn groß).{" "}
                {bothModes
                  ? "Je Feld stehen LAN- und Cloud-Adresse bereit – die passende für die jeweilige Halle wählen."
                  : cloudOnly
                    ? "Tablet und PC brauchen je eine Internet-Verbindung – kein gemeinsames WLAN nötig."
                    : "Tablet und dieser PC müssen im selben WLAN sein."}
              </p>
              {courtGroups.map((g) => (
                <div key={g.location} className="flex flex-col gap-2">
                  <HallHeading name={g.location} />
                  <div
                    className={`grid grid-cols-1 gap-2 ${
                      bothModes ? "" : "sm:grid-cols-2"
                    }`}
                  >
                    {g.courts.map((c) => (
                      <QrCard
                        key={c.court_id}
                        court={c}
                        addresses={courtAddresses(c.court_id)}
                        onZoom={(address) => setZoom({ court: c, address })}
                      />
                    ))}
                  </div>
                </div>
              ))}
            </section>
          )}
        </>
      )}

      {zoom !== null && (
        <div
          onClick={() => setZoom(null)}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-6"
        >
          <div className="flex flex-col items-center rounded-xl bg-white p-6 text-center">
            <img
              src={zoom.address.qrUrl}
              alt=""
              className="bg-white"
              style={{ width: "min(72vw, 72vh)", height: "min(72vw, 72vh)" }}
            />
            <div className="mt-3 flex items-center gap-2 text-lg font-semibold">
              {zoom.court.court}
              {bothModes && <AddressBadge kind={zoom.address.kind} />}
            </div>
            <div className="mt-1 max-w-[20rem] break-all text-sm text-slate-500">
              {zoom.address.courtUrl}
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

/**
 * Gruppiert die Felder hallenweise: eine Gruppe je distinktem `location`-Wert
 * (in Erst-Vorkommens-Reihenfolge). Felder ohne Halle landen in einer Gruppe
 * mit leerem Namen, die ans Ende sortiert wird – so geht kein Feld verloren.
 * Bei einem Ein-Hallen-Turnier entsteht genau diese eine namenlose Gruppe,
 * der Aufrufer rendert sie ohne Überschrift wie ein flaches Raster.
 */
function groupByHall(
  courts: CourtOverview[],
): { location: string; courts: CourtOverview[] }[] {
  const groups: { location: string; courts: CourtOverview[] }[] = [];
  for (const c of courts) {
    const loc = c.location || "";
    let g = groups.find((x) => x.location === loc);
    if (!g) {
      g = { location: loc, courts: [] };
      groups.push(g);
    }
    g.courts.push(c);
  }
  // Gruppe ohne Halle ans Ende (stabile Sortierung lässt die Hallen-
  // Reihenfolge sonst unangetastet).
  groups.sort((a, b) => Number(a.location === "") - Number(b.location === ""));
  return groups;
}

/** Hallen-Überschrift über einer Feld-Gruppe; ohne Hallenname (leer) nichts. */
function HallHeading({ name }: { name: string }) {
  if (!name) return null;
  return (
    <h3 className="mt-1 text-sm font-semibold text-slate-600">{name}</h3>
  );
}

/**
 * Eine QR-Code-Karte für ein Feld. `addresses` ist die Liste der
 * Verbindungswege: im Einzelmodus ein Eintrag (rendert exakt wie zuvor),
 * im Doppelmodus zwei – dann je Weg ein QR-Code mit „LAN"-/„Cloud"-Badge.
 */
function QrCard({
  court,
  addresses,
  onZoom,
}: {
  court: CourtOverview;
  addresses: CourtAddress[];
  onZoom: (address: CourtAddress) => void;
}) {
  const both = addresses.length > 1;
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-slate-200 bg-white p-2 shadow-sm">
      <div className="truncate px-1 text-sm font-medium">{court.court}</div>
      {addresses.map((addr) => (
        <div key={addr.kind} className="flex items-center gap-3">
          <button
            onClick={() => onZoom(addr)}
            title="QR-Code groß anzeigen"
            className="shrink-0 rounded bg-white"
          >
            <img
              src={addr.qrUrl}
              alt=""
              width={64}
              height={64}
              className="block"
            />
          </button>
          <div className="min-w-0 flex-1">
            {both && <AddressBadge kind={addr.kind} />}
            <div className="truncate text-xs text-slate-500">
              {addr.courtUrl}
            </div>
          </div>
          <CopyUrlButton url={addr.courtUrl} />
        </div>
      ))}
    </div>
  );
}

/** Kleines „LAN"-/„Cloud"-Badge zur Kennzeichnung eines Verbindungswegs. */
function AddressBadge({ kind }: { kind: "lan" | "cloud" }) {
  const cloud = kind === "cloud";
  return (
    <span
      className={`mb-0.5 inline-flex items-center gap-1 rounded-full px-2 py-0.5
                  text-[10px] font-medium ${
                    cloud
                      ? "bg-sky-100 text-sky-700"
                      : "bg-slate-200 text-slate-600"
                  }`}
    >
      {cloud ? <Cloud size={11} /> : <Wifi size={11} />}
      {cloud ? "Cloud" : "LAN"}
    </span>
  );
}

/**
 * Verbindungsart-Badge im Seitenkopf. Zeigt im Doppelmodus „LAN + Cloud",
 * sonst genau einen Weg – wie bisher.
 */
function ModeBadge({
  lanEnabled,
  cloudEnabled,
}: {
  lanEnabled: boolean;
  cloudEnabled: boolean;
}) {
  const both = lanEnabled && cloudEnabled;
  const cloudOnly = cloudEnabled && !lanEnabled;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1
                  text-xs font-medium ${
                    cloudOnly
                      ? "bg-sky-100 text-sky-700"
                      : "bg-slate-200 text-slate-600"
                  }`}
      title={
        both
          ? "Doppelmodus: Tablets verbinden per LAN und über badhub.de"
          : cloudOnly
            ? "Cloud-Modus: Tablets verbinden über badhub.de"
            : "LAN-Modus: Tablets verbinden im lokalen Netz"
      }
    >
      {cloudOnly ? <Cloud size={14} /> : <Wifi size={14} />}
      {both ? "LAN + Cloud" : cloudOnly ? "Cloud" : "LAN"}
    </span>
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
          {court.scorekeeper.length > 0 && (
            <div
              className="mt-1 flex items-center gap-1 truncate text-xs text-slate-400"
              title="Zähltafelbediener (Verlierer des Vorspiels)"
            >
              <Tablet size={12} className="shrink-0" />
              <span className="truncate">{court.scorekeeper.join(" / ")}</span>
            </div>
          )}
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
