import { useEffect, useState } from "react";
import {
  ListOrdered,
  type LucideIcon,
  Monitor,
  Radio,
  Volume2,
} from "lucide-react";
import {
  monitorDevices,
  openExternal,
  openLiveView,
  saveConfig,
  tabletOverview,
  tournamentStats,
} from "../api";
import { SlaveDevicesPanel } from "../components/SlaveDevicesPanel";
import type { NavView, SettingsFocus } from "../components/SideNav";
import type {
  AppConfig,
  CourtOverview,
  MonitorDeviceInfo,
  SyncStatus,
  TournamentStats,
} from "../types";

interface Props {
  config: AppConfig;
  /** Live-Status – kommt von App (geteilt mit der Kopfzeile). */
  status: SyncStatus | null;
  /** Navigation in andere Bereiche (z. B. Einstellungen → Ansagen). */
  onNavigate?: (view: NavView, focus?: SettingsFocus) => void;
  /** Speichern der Config (für die Ansage-Halle-Schnellwahl direkt hier). */
  onConfigSaved?: (config: AppConfig) => void;
}

function dotColor(status: SyncStatus): string {
  if (!status.running) return "bg-slate-400";
  if (status.kind === "ok") return "bg-emerald-500";
  if (status.kind === "idle") return "bg-amber-400";
  return "bg-rose-500";
}

function ago(ms: number): string {
  if (ms === 0) return "–";
  const secs = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (secs < 60) return `vor ${secs} s`;
  return `vor ${Math.round(secs / 60)} min`;
}

/** Einheitlicher Aktions-Button mit Icon, Beschriftung und Tooltip. */
function ActionButton(props: {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
}) {
  const Icon = props.icon;
  return (
    <button
      onClick={props.onClick}
      disabled={props.disabled}
      title={props.title}
      className="inline-flex items-center gap-2 rounded-lg bg-slate-100 px-3.5 py-2 text-sm
                 font-medium text-slate-700 transition-colors hover:bg-slate-200
                 disabled:opacity-50"
    >
      <Icon size={16} strokeWidth={2} />
      {props.label}
    </button>
  );
}

/** Eine Kennzahl-Kachel (große Zahl + Beschriftung), BTP-Vorbild. */
function StatCard(props: { value: string | number; label: string }) {
  return (
    <div className="rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
      <div className="text-2xl font-semibold leading-none text-slate-800">
        {props.value}
      </div>
      <div className="mt-1 text-xs font-medium uppercase tracking-wide text-slate-400">
        {props.label}
      </div>
    </div>
  );
}

/** Abdeckungs-Balken „X / Y Felder" — voll = grün, sonst gelb (unvollständig). */
function CoverageBar(props: {
  label: string;
  covered: number;
  total: number;
  note?: string;
}) {
  const pct = props.total > 0 ? Math.round((props.covered / props.total) * 100) : 0;
  const full = props.total > 0 && props.covered >= props.total;
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-baseline justify-between">
        <span className="text-sm font-medium text-slate-700">{props.label}</span>
        <span className="text-sm text-slate-500">
          {props.covered} / {props.total} Felder
          {props.note ? <span className="text-slate-400"> · {props.note}</span> : null}
        </span>
      </div>
      <div className="h-2.5 w-full overflow-hidden rounded-full bg-slate-100">
        <div
          className={`h-full rounded-full transition-all ${full ? "bg-emerald-500" : "bg-amber-400"}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

export function Dashboard({ config, status, onNavigate, onConfigSaved }: Props) {
  const running = status?.running ?? false;
  // Turnier-Kennzahlen + Hallen sind erst NACH dem Start bekannt (dann steht
  // die BTP-Verbindung und die Turnierdatei ist geladen).
  const [stats, setStats] = useState<TournamentStats | null>(null);
  const [halls, setHalls] = useState<string[]>([]);
  // Felder (für Tablet-Abdeckung) + Monitor-Geräte (für TV-Abdeckung).
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  const [monitors, setMonitors] = useState<MonitorDeviceInfo[]>([]);
  const [savingHall, setSavingHall] = useState(false);
  const [hallSaveError, setHallSaveError] = useState(false);

  useEffect(() => {
    if (!running) {
      setStats(null);
      setHalls([]);
      setCourts([]);
      setMonitors([]);
      return;
    }
    let active = true;
    const load = () => {
      tournamentStats()
        .then((s) => {
          if (!active) return;
          setStats(s);
          if (s) setHalls(s.halls);
        })
        .catch(() => {});
      // Court-Übersicht: Hallen-Fallback + Tablet-Abdeckung (tablet_connected).
      tabletOverview()
        .then((info) => {
          if (!active) return;
          setCourts(info.courts ?? []);
          setHalls((prev) =>
            prev.length > 0
              ? prev
              : [
                  ...new Set(
                    (info.courts ?? [])
                      .map((c) => c.location)
                      .filter((l) => l !== ""),
                  ),
                ].sort((a, b) => a.localeCompare(b, "de")),
          );
        })
        .catch(() => {});
      // Monitor-Geräte für die TV-Abdeckung (einzeln/kombi).
      monitorDevices()
        .then((m) => {
          if (active) setMonitors(m);
        })
        .catch(() => {});
    };
    load();
    const id = setInterval(load, 15000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [running]);

  // Ansage-Halle direkt hier umstellen (statt unten in den Einstellungen).
  async function changeAnnounceHall(hall: string) {
    setSavingHall(true);
    setHallSaveError(false);
    try {
      const next: AppConfig = {
        ...config,
        announce: { ...config.announce, announce_hall: hall },
      };
      await saveConfig(next);
      onConfigSaved?.(next);
    } catch {
      // Speichern fehlgeschlagen – Auswahl springt (über die kontrollierte
      // Select-Bindung) auf den alten Wert zurück, Hinweis anzeigen.
      setHallSaveError(true);
    } finally {
      setSavingHall(false);
    }
  }

  if (!status) {
    return (
      <main className="flex h-full items-center justify-center text-slate-400">
        Lädt …
      </main>
    );
  }

  const multiHall = halls.length >= 2;
  const finishedPct =
    stats && stats.matches_total > 0
      ? Math.round((stats.matches_finished / stats.matches_total) * 100)
      : 0;

  // Geräte-Abdeckung: wie viele Felder haben ein Tablet bzw. einen TV?
  const courtIdSet = new Set(courts.map((c) => c.court_id));
  const totalCourts = courts.length;
  const tabletCovered = courts.filter((c) => c.tablet_connected).length;
  // TV-Abdeckung aus den Monitor-Zuweisungen (Einzel + Kombi); nur Felder des
  // Turniers zählen (ein Gerät könnte eine veraltete CourtID tragen).
  const tvCovered = new Set<number>();
  const tvCombo = new Set<number>();
  let tvOfflineAssigned = 0;
  for (const m of monitors) {
    const t = m.target;
    const ids =
      t?.kind === "court"
        ? [t.court_id]
        : t?.kind === "court_combo"
          ? t.court_ids
          : [];
    if (ids.length === 0) continue;
    if (!m.online) tvOfflineAssigned += 1;
    for (const id of ids) {
      if (!courtIdSet.has(id)) continue;
      tvCovered.add(id);
      if (t?.kind === "court_combo") tvCombo.add(id);
    }
  }
  const tvNoteParts: string[] = [];
  if (tvCombo.size > 0) tvNoteParts.push(`${tvCombo.size} in Kombi`);
  if (tvOfflineAssigned > 0) tvNoteParts.push(`${tvOfflineAssigned} offline`);

  return (
    <main className="mx-auto flex min-h-full max-w-2xl flex-col gap-5 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold leading-tight">
          {stats?.tournament_name.trim() || "Dashboard"}
        </h1>
        <p className="text-sm text-slate-500">
          {stats?.tournament_name.trim()
            ? "Turnier-Übersicht"
            : "Turnier-Übersicht & Liveticker-Status"}
        </p>
      </header>

      {/* Ferne Halle (Cloud-Slave): Tablet-QR + Monitor-Links der eigenen
          Halle. Rendert nur, wenn dieser PC als Cloud-Slave läuft. */}
      {config.slave_mode && <SlaveDevicesPanel />}

      {/* Liveticker-Status */}
      <section className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
        <div className="flex items-center gap-2.5">
          <span
            className={`h-3 w-3 rounded-full ${dotColor(status)} ${
              status.running && status.kind === "ok" ? "animate-pulse" : ""
            }`}
          />
          <span className="font-medium">
            {status.running ? "Liveticker aktiv" : "Gestoppt"}
          </span>
        </div>
        <p className="mt-2 text-sm text-slate-600">{status.message}</p>
        <p className="mt-1 text-xs text-slate-400">
          Letzter Stand: {ago(status.updated_at_ms)}
        </p>
      </section>

      {/* Turnier-Kennzahlen (nur sinnvoll, wenn der Liveticker läuft). */}
      {running && stats && (
        <section className="flex flex-col gap-3">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            <StatCard value={stats.n_disciplines} label="Konkurrenzen" />
            <StatCard value={stats.n_players} label="Spieler" />
            <StatCard value={stats.matches_total} label="Spiele" />
            <StatCard value={stats.n_courts} label="Felder" />
            <StatCard value={stats.matches_running} label="Laufend" />
            <StatCard value={halls.length || 1} label="Hallen" />
          </div>

          {/* Spiel-Fortschritt: abgeschlossen / gesamt */}
          <div className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
            <div className="flex items-baseline justify-between">
              <span className="text-sm font-medium text-slate-700">
                Abgeschlossene Spiele
              </span>
              <span className="text-sm text-slate-500">
                {stats.matches_finished} / {stats.matches_total} ({finishedPct}
                %)
              </span>
            </div>
            <div className="mt-2 h-2.5 w-full overflow-hidden rounded-full bg-slate-100">
              <div
                className="h-full rounded-full bg-emerald-500 transition-all"
                style={{ width: `${finishedPct}%` }}
              />
            </div>
          </div>

          {/* Geräte-Abdeckung: Tablet + TV je Feld. */}
          {totalCourts > 0 && (
            <div className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
              <CoverageBar
                label="Tablets"
                covered={tabletCovered}
                total={totalCourts}
              />
              <CoverageBar
                label="Monitore (TV)"
                covered={tvCovered.size}
                total={totalCourts}
                note={tvNoteParts.join(", ") || undefined}
              />
            </div>
          )}

          {halls.length > 0 && (
            <p className="text-xs text-slate-500">
              Hallen: <span className="font-medium">{halls.join(", ")}</span>
            </p>
          )}
        </section>
      )}

      {running && !stats && (
        <p className="text-xs text-slate-400">
          Turnierdaten werden geladen …
        </p>
      )}

      {/* Mehr-Hallen-Schnellwahl: Ansage-Halle direkt hier setzen (gespeichert
          wird sofort) — kein Scrollen ans Ende der Einstellungen mehr. */}
      {multiHall && (
        <section className="flex flex-col gap-2 rounded-xl border border-amber-300 bg-amber-50 p-5 shadow-sm">
          <div className="flex items-center gap-2 font-medium text-amber-900">
            <Volume2 size={18} className="shrink-0" />
            Mehr-Hallen-Turnier erkannt
          </div>
          <p className="text-sm text-amber-800">
            Lege fest, welche Halle dieser PC ansagt, damit jede Halle nur ihre
            eigenen Ansagen hört.
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <label className="text-sm font-medium text-amber-900">
              Dieser PC sagt an:
            </label>
            <select
              value={config.announce.announce_hall}
              disabled={savingHall}
              onChange={(e) => void changeAnnounceHall(e.currentTarget.value)}
              className="rounded-lg border border-amber-300 bg-white px-2.5 py-1.5 text-sm
                         text-slate-800 focus:border-amber-500 focus:outline-none
                         disabled:opacity-50"
            >
              <option value="">Alle Hallen</option>
              {halls.map((h) => (
                <option key={h} value={h}>
                  nur {h}
                </option>
              ))}
            </select>
            {savingHall && (
              <span className="text-xs text-amber-700">Speichere …</span>
            )}
            {hallSaveError && (
              <span className="text-xs font-medium text-rose-600">
                Speichern fehlgeschlagen
              </span>
            )}
          </div>
          {!config.announce.enabled && (
            <p className="text-sm text-amber-800">
              Ansagen sind aktuell deaktiviert.{" "}
              <button
                onClick={() => onNavigate?.("settings", "ansagen")}
                className="font-medium underline underline-offset-2 hover:text-amber-900"
              >
                In den Einstellungen aktivieren
              </button>
            </p>
          )}
        </section>
      )}

      {config.badhub.live_url !== "" && (
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-slate-700">
            Anzeigen im Browser öffnen
          </h2>
          <div className="flex flex-wrap gap-2">
            <ActionButton
              icon={Radio}
              label="Liveticker"
              onClick={() => openLiveView(null)}
              disabled={!running}
              title={
                running
                  ? "Öffentliche Liveticker-Seite im Browser öffnen"
                  : "Erst nach Start des Livetickers verfügbar"
              }
            />
            <ActionButton
              icon={Monitor}
              label="Hallen-Monitor"
              onClick={() => openLiveView("monitor")}
              disabled={!running}
              title={
                running
                  ? "Großbild-Ansicht für einen Hallen-Monitor (online)"
                  : "Erst nach Start des Livetickers verfügbar"
              }
            />
            <ActionButton
              icon={ListOrdered}
              label="Nächste Spiele"
              onClick={() => openLiveView("next")}
              disabled={!running}
              title={
                running
                  ? "Aufruf-Anzeige der als Nächstes anstehenden Spiele"
                  : "Erst nach Start des Livetickers verfügbar"
              }
            />
          </div>
          {!running && (
            <p className="text-xs text-slate-400">
              Verfügbar, sobald der Liveticker gestartet ist (dann steht die
              Verbindung zu BTP).
            </p>
          )}
          {/* Ab 2 Hallen je Halle ein lokaler Hallen-Monitor (Court-Übersicht
              dieser Halle) – am PC im Browser geöffnet. */}
          {running && multiHall && (
            <div className="mt-1 flex flex-col gap-1.5">
              <span className="text-xs text-slate-500">
                Hallen-Monitor je Halle (lokal):
              </span>
              <div className="flex flex-wrap gap-2">
                {halls.map((hall) => (
                  <ActionButton
                    key={hall}
                    icon={Monitor}
                    label={hall}
                    onClick={() =>
                      void openExternal(
                        `http://localhost:8088/info/overview?halle=${encodeURIComponent(hall)}`,
                      )
                    }
                    title={`Lokale Court-Übersicht für ${hall} im Browser öffnen`}
                  />
                ))}
              </div>
            </div>
          )}
        </section>
      )}
    </main>
  );
}
