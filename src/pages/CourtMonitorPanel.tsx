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
import {
  assignMonitor,
  listCourtAds,
  monitorCommand,
  monitorDevices,
  tabletOverview,
} from "../api";
import type {
  CourtAd,
  CourtOverview,
  MonitorDeviceInfo,
  MonitorTarget,
  TabletInfo,
} from "../types";

// ─── String ↔ MonitorTarget-Konvertierung fürs <select> ──────────────────
// <option value="…"> muss ein String sein. Schlüssel:
//   ""                       → keine Zuweisung
//   "court:<id>"             → MonitorTarget::Court { court_id }
//   "info_overview"          → MonitorTarget::InfoOverview
//   "info_preparation"       → MonitorTarget::InfoPreparation
//   "ad_rotation"            → MonitorTarget::AdRotation
//   "ad_single:<dateiname>"  → MonitorTarget::AdSingle { file }
//   "combo:1,2,3"            → MonitorTarget::CourtCombo { court_ids }
//   "__combo_edit__"         → öffnet den Kombi-Dialog (kein echtes Target)

function targetToValue(t: MonitorTarget | null): string {
  if (!t) return "";
  if (t.kind === "court") return `court:${t.court_id}`;
  if (t.kind === "ad_single") return `ad_single:${t.file}`;
  if (t.kind === "court_combo") return `combo:${t.court_ids.join(",")}`;
  return t.kind;
}

function valueToTarget(v: string): MonitorTarget | null {
  if (v === "") return null;
  if (v === "info_overview") return { kind: "info_overview" };
  if (v === "info_preparation") return { kind: "info_preparation" };
  if (v === "ad_rotation") return { kind: "ad_rotation" };
  if (v.startsWith("court:")) {
    const id = Number(v.slice("court:".length));
    if (Number.isFinite(id)) return { kind: "court", court_id: id };
  }
  if (v.startsWith("ad_single:")) {
    const file = v.slice("ad_single:".length);
    if (file.length > 0) return { kind: "ad_single", file };
  }
  if (v.startsWith("combo:")) {
    const ids = v
      .slice("combo:".length)
      .split(",")
      .map((s) => Number(s))
      .filter((n) => Number.isFinite(n));
    if (ids.length > 0) return { kind: "court_combo", court_ids: ids };
  }
  return null;
}

// ─── Sortier-/Gruppier-Logik der Geräteliste ────────────────────────────
// Eine fertig sortierte Anzeige-Struktur: Online-Block (ggf. nach Hallen
// unterteilt) + Offline-Block unter einer Trennlinie. Innerhalb jeder
// Sektion ist nach Typ + Feldnummer sortiert.

interface DeviceGroup {
  /** Hallenname; leer wenn keine/Einzelhalle bzw. Info-/unzugewiesene Geräte. */
  hall: string;
  devices: MonitorDeviceInfo[];
}
interface GroupedDevices {
  online: DeviceGroup[];
  offline: DeviceGroup[];
  /** Hallennamen anzeigen? Nur bei ≥2 distinkten, nicht-leeren Hallen. */
  showHalls: boolean;
}

/** Typ-Rang fürs Sortieren: Feld < Kombi < Info/Werbung < unzugewiesen. */
function targetRank(t: MonitorTarget | null): number {
  if (!t) return 3;
  if (t.kind === "court") return 0;
  if (t.kind === "court_combo") return 1;
  return 2; // info_overview / info_preparation / ad_*
}

/** Feld-Sortierschlüssel: numerischer Anteil des Feldnamens (Feld 1 zuerst);
 *  ohne Zahl ans Ende. */
function fieldSortKey(name: string | null): number {
  if (!name) return Number.MAX_SAFE_INTEGER;
  const m = name.match(/\d+/);
  return m ? parseInt(m[0], 10) : Number.MAX_SAFE_INTEGER;
}

function groupDevicesForDisplay(
  devices: MonitorDeviceInfo[],
  courts: CourtOverview[],
): GroupedDevices {
  const courtById = new Map(courts.map((c) => [c.court_id, c]));
  // Halle eines Geräts: aus seinem (ersten) zugewiesenen Feld. Info-/
  // Werbe-/Kombi-/unzugewiesene Geräte haben keine eindeutige Halle.
  const hallOf = (d: MonitorDeviceInfo): string => {
    const cid =
      d.target?.kind === "court"
        ? d.target.court_id
        : d.courtId !== null
          ? d.courtId
          : null;
    if (cid === null) return "";
    return courtById.get(cid)?.location ?? "";
  };

  const distinctHalls = new Set(
    devices.map(hallOf).filter((h) => h !== ""),
  );
  const showHalls = distinctHalls.size >= 2;

  const sortWithin = (a: MonitorDeviceInfo, b: MonitorDeviceInfo): number => {
    const r = targetRank(a.target) - targetRank(b.target);
    if (r !== 0) return r;
    const f = fieldSortKey(a.court) - fieldSortKey(b.court);
    if (f !== 0) return f;
    return a.code.localeCompare(b.code);
  };

  const buildGroups = (list: MonitorDeviceInfo[]): DeviceGroup[] => {
    if (!showHalls) {
      return list.length ? [{ hall: "", devices: [...list].sort(sortWithin) }] : [];
    }
    // Nach Halle bündeln; Geräte ohne Halle (Info/Kombi/unzugewiesen)
    // landen in einer Rest-Gruppe am Ende.
    const byHall = new Map<string, MonitorDeviceInfo[]>();
    for (const d of list) {
      const h = hallOf(d);
      if (!byHall.has(h)) byHall.set(h, []);
      byHall.get(h)!.push(d);
    }
    const halls = [...byHall.keys()].sort((a, b) => {
      if (a === "") return 1; // Rest-Gruppe ans Ende
      if (b === "") return -1;
      return a.localeCompare(b, "de");
    });
    return halls.map((h) => ({ hall: h, devices: byHall.get(h)!.sort(sortWithin) }));
  };

  return {
    online: buildGroups(devices.filter((d) => d.online)),
    offline: buildGroups(devices.filter((d) => !d.online)),
    showHalls,
  };
}

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
  // Werbebild-Liste fuer das "Werbung"-optgroup. Polling im selben Takt
  // wie die Geraeteliste, damit das Dropdown sich live aktualisiert,
  // wenn der Operator parallel in den Einstellungen ein Bild
  // hinzufuegt oder entfernt.
  const [ads, setAds] = useState<CourtAd[]>([]);

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
      listCourtAds()
        .then((a) => {
          if (active) setAds(a);
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

  // LAN und Cloud sind unabhängig schaltbar – im Doppelmodus beide aktiv.
  const lanEnabled = info?.lan_enabled ?? true;
  const cloudEnabled = info?.cloud_enabled ?? false;
  const cloudOnly = cloudEnabled && !lanEnabled;
  const bothModes = lanEnabled && cloudEnabled;
  // Im LAN-Pfad der feste mDNS-Name (muss zu MDNS_HOST + TABLET_PORT im
  // Rust-Kern passen) – so braucht es keine feste IP. Die IP-Adresse dient
  // nur als Rückfall, falls der Name im Netz nicht aufgelöst wird.
  const lanMonitorUrl = "http://bts-light.local:8088/monitor";
  const cloudMonitorUrl = `${info?.relay_base ?? ""}/monitor`;
  const fallbackUrl =
    lanEnabled && info?.server_host ? `http://${info.server_host}/monitor` : "";
  // Felder mit Identität (CourtID) und Anzeigename – die Zuweisung nutzt
  // die CourtID, das <select> zeigt den Namen.
  const courts: CourtOverview[] = info?.courts ?? [];

  // Geräte für die Anzeige sortieren + gruppieren:
  //  - online zuerst, offline darunter (Trennlinie)
  //  - je Block nach Halle gruppiert (nur wenn ≥2 Hallen)
  //  - innerhalb: Felder (nach Feld-Nr.) → Kombi-Felder → Info/Werbung →
  //    unzugewiesen
  const grouped = groupDevicesForDisplay(devices, courts);

  async function refresh() {
    try {
      setDevices(await monitorDevices());
    } catch {
      /* ignorieren – nächster Poll versucht es erneut */
    }
  }

  async function assign(deviceId: string, target: MonitorTarget | null) {
    try {
      await assignMonitor(deviceId, target);
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
                        cloudOnly
                          ? "bg-sky-100 text-sky-700"
                          : "bg-slate-200 text-slate-600"
                      }`}
        >
          {cloudOnly ? <Cloud size={14} /> : <Wifi size={14} />}
          {bothModes ? "LAN + Cloud" : cloudOnly ? "Cloud" : "LAN"}
        </span>
      </header>

      {/* Einrichtungs-Adresse(n) für alle Pis */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Einrichtung am Raspberry Pi
        </h2>
        <p className="text-xs text-slate-500">
          {bothModes
            ? "Je nach Halle die LAN- oder die Cloud-Adresse im Chromium-Kiosk öffnen. Das Gerät zeigt dann einen Code; ordne es unten einem Feld zu."
            : "Alle Monitore bekommen "}
          {!bothModes && (
            <>
              <span className="font-medium">dieselbe</span> Adresse – im
              Chromium-Kiosk öffnen. Das Gerät zeigt dann einen Code; ordne es
              unten einem Feld zu.
            </>
          )}
        </p>
        {/* Im Doppelmodus beide Adressen mit Badge, sonst genau eine –
            das Layout bleibt im Einzelmodus unverändert. */}
        {lanEnabled && (
          <MonitorAddressRow
            url={lanMonitorUrl}
            kind="lan"
            showBadge={bothModes}
          />
        )}
        {cloudEnabled && (
          <MonitorAddressRow
            url={cloudMonitorUrl}
            kind="cloud"
            showBadge={bothModes}
          />
        )}
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
            {/* Online-Geräte, nach Halle gruppiert + nach Feld sortiert. */}
            {grouped.online.map((g) => (
              <div key={`on-${g.hall || "_"}`} className="flex flex-col gap-2">
                {grouped.showHalls && g.hall && (
                  <h3 className="mt-1 text-xs font-bold uppercase tracking-wide text-slate-500">
                    {g.hall}
                  </h3>
                )}
                {g.devices.map((d) => (
                  <DeviceRow
                    key={d.id}
                    device={d}
                    courts={courts}
                    ads={ads}
                    onAssign={(target) => void assign(d.id, target)}
                    onIdentify={() => void monitorCommand(d.id, "identify")}
                    onReload={() => void monitorCommand(d.id, "reload")}
                  />
                ))}
              </div>
            ))}

            {/* Offline-Geräte: unter einer Trennlinie, gleiche Sortierung. */}
            {grouped.offline.some((g) => g.devices.length > 0) && (
              <div className="mt-2 flex items-center gap-2">
                <div className="h-px flex-1 bg-slate-200" />
                <span className="text-xs font-medium text-slate-400">
                  offline
                </span>
                <div className="h-px flex-1 bg-slate-200" />
              </div>
            )}
            {grouped.offline.map((g) => (
              <div key={`off-${g.hall || "_"}`} className="flex flex-col gap-2 opacity-60">
                {grouped.showHalls && g.hall && (
                  <h3 className="mt-1 text-xs font-bold uppercase tracking-wide text-slate-400">
                    {g.hall}
                  </h3>
                )}
                {g.devices.map((d) => (
                  <DeviceRow
                    key={d.id}
                    device={d}
                    courts={courts}
                    ads={ads}
                    onAssign={(target) => void assign(d.id, target)}
                    onIdentify={() => void monitorCommand(d.id, "identify")}
                    onReload={() => void monitorCommand(d.id, "reload")}
                  />
                ))}
              </div>
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
  ads,
  onAssign,
  onIdentify,
  onReload,
}: {
  device: MonitorDeviceInfo;
  courts: CourtOverview[];
  ads: CourtAd[];
  onAssign: (target: MonitorTarget | null) => void;
  onIdentify: () => void;
  onReload: () => void;
}) {
  // Optionen des <select>: value = String-Schlüssel ("" = keine,
  // "court:<id>", "info_overview", "info_preparation"), Text = Anzeige.
  // Falls einem Gerät ein Feld zugewiesen ist, das nicht (mehr) in der
  // Court-Liste steht, trotzdem als Option führen.
  const fieldOptions: { id: number; label: string; location: string }[] =
    courts.map((c) => ({
      id: c.court_id,
      label: c.court,
      location: c.location,
    }));
  if (
    device.courtId !== null &&
    !fieldOptions.some((o) => o.id === device.courtId)
  ) {
    fieldOptions.unshift({
      id: device.courtId,
      label: device.court ?? `Feld ${device.courtId}`,
      location: "",
    });
  }
  // Mehr-Hallen-Turnier (≥2 distinkte, nicht-leere Hallennamen): die
  // <option>s pro Halle in <optgroup> bündeln. Sonst flache Liste.
  const hallNames = [
    ...new Set(fieldOptions.map((o) => o.location).filter((l) => l !== "")),
  ];
  const grouped = hallNames.length >= 2;

  // Aktueller String-Wert für das <select> (eindeutiger Schlüssel).
  const currentValue = targetToValue(device.target);

  // Kombi-Dialog (mehrere Felder auf einem TV).
  const [comboOpen, setComboOpen] = useState(false);

  function onChange(value: string) {
    if (value === "__combo_edit__") {
      setComboOpen(true);
      return; // kein echtes Target — Dialog übernimmt
    }
    onAssign(valueToTarget(value));
  }

  // Label der aktuellen Kombi-Zuweisung (für die Dropdown-Option).
  const comboCourtIds =
    device.target?.kind === "court_combo" ? device.target.court_ids : null;
  const comboLabel = comboCourtIds
    ? "Kombi: " +
      comboCourtIds
        .map((id) => fieldOptions.find((o) => o.id === id)?.label ?? id)
        .join(" + ")
    : null;

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
        value={currentValue}
        onChange={(e) => onChange(e.currentTarget.value)}
        className="ml-auto rounded-lg border border-slate-300 bg-white px-2.5 py-1.5 text-sm
                   focus:border-slate-500 focus:outline-none"
      >
        <option value="">— keine Zuweisung —</option>
        {grouped ? (
          <>
            {hallNames.map((hall) => (
              <optgroup key={hall} label={hall}>
                {fieldOptions
                  .filter((o) => o.location === hall)
                  .map((o) => (
                    <option key={o.id} value={`court:${o.id}`}>
                      {o.label}
                    </option>
                  ))}
              </optgroup>
            ))}
            {/* Felder ohne auflösbare Halle (z. B. nach Turnierwechsel)
                bleiben ohne <optgroup> erhalten, damit keine Zuweisung
                aus der Liste verschwindet. */}
            {fieldOptions
              .filter((o) => o.location === "")
              .map((o) => (
                <option key={o.id} value={`court:${o.id}`}>
                  {o.label}
                </option>
              ))}
          </>
        ) : (
          fieldOptions.map((o) => (
            <option key={o.id} value={`court:${o.id}`}>
              {o.label}
            </option>
          ))
        )}
        {/* Info-Monitore: Hallen-weite Read-Only-Anzeigen ohne Feld-Bezug. */}
        <optgroup label="Informationen">
          <option value="info_overview">Court-Übersicht</option>
          <option value="info_preparation">In Vorbereitung</option>
        </optgroup>
        {/* Werbe-Anzeige: rotierend oder Einzelbild. Wenn keine Werbe-
            bilder hinterlegt sind, ist die Gruppe deaktiviert (das HTML
            erlaubt `disabled` auf optgroup). Eine bereits zugewiesene
            ad_single-Datei taucht zusätzlich oben mit auf, damit die
            Auswahl sichtbar bleibt, selbst wenn die Datei zwischenzeitlich
            aus dem Pool entfernt wurde. */}
        <optgroup label="Werbung" disabled={ads.length === 0}>
          {ads.length === 0 ? (
            <option value="" disabled>
              — keine Werbebilder hinterlegt —
            </option>
          ) : (
            <>
              <option value="ad_rotation">
                Rotierend ({ads.length}{" "}
                {ads.length === 1 ? "Bild" : "Bilder"})
              </option>
              {ads.map((ad) => (
                <option key={ad.file} value={`ad_single:${ad.file}`}>
                  {ad.label || ad.file}
                </option>
              ))}
              {/* Falls die gerade zugewiesene Datei nicht (mehr) im Pool
                  steckt, dennoch als Option aufnehmen — sonst rutschte die
                  Auswahl unsichtbar aus dem Dropdown. */}
              {(() => {
                const t = device.target;
                if (
                  t?.kind === "ad_single" &&
                  !ads.some((a) => a.file === t.file)
                ) {
                  return (
                    <option
                      value={`ad_single:${t.file}`}
                    >{`${t.file} (nicht mehr im Pool)`}</option>
                  );
                }
                return null;
              })()}
            </>
          )}
        </optgroup>
        {/* Kombi-Anzeige: mehrere Felder auf einem TV. Die aktuelle
            Kombi-Auswahl wird als eigene (selektierte) Option gezeigt;
            „Felder wählen…" öffnet den Dialog. Mind. 2 Felder nötig. */}
        <optgroup label="Kombi-Anzeige" disabled={fieldOptions.length < 2}>
          {comboLabel && (
            <option value={currentValue}>{comboLabel}</option>
          )}
          <option value="__combo_edit__">
            {comboLabel ? "Felder ändern…" : "Felder wählen…"}
          </option>
        </optgroup>
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

      {comboOpen && (
        <ComboDialog
          fields={fieldOptions}
          initial={comboCourtIds ?? []}
          onCancel={() => setComboOpen(false)}
          onConfirm={(ids) => {
            setComboOpen(false);
            onAssign({ kind: "court_combo", court_ids: ids });
          }}
        />
      )}
    </div>
  );
}

/**
 * Modaler Dialog zur Auswahl von 2-3 Feldern für die Kombi-Anzeige.
 * `fields` sind die wählbaren Felder, `initial` die aktuell gewählten
 * CourtIDs. Die Auswahl-Reihenfolge bestimmt die Band-Reihenfolge auf
 * dem TV; max. 3 Felder (mehr wird auf 16:9 unleserlich).
 */
function ComboDialog({
  fields,
  initial,
  onCancel,
  onConfirm,
}: {
  fields: { id: number; label: string; location: string }[];
  initial: number[];
  onCancel: () => void;
  onConfirm: (ids: number[]) => void;
}) {
  const MAX = 3;
  // Auswahl als geordnete Liste (Reihenfolge = Band-Reihenfolge).
  const [selected, setSelected] = useState<number[]>(initial);

  function toggle(id: number) {
    setSelected((prev) => {
      if (prev.includes(id)) return prev.filter((x) => x !== id);
      if (prev.length >= MAX) return prev; // Cap: nicht mehr als 3
      return [...prev, id];
    });
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      onClick={onCancel}
    >
      <div
        className="w-full max-w-md rounded-xl bg-white p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-lg font-semibold text-slate-800">
          Kombi-Anzeige — Felder wählen
        </h3>
        <p className="mt-1 text-sm text-slate-500">
          Wähle 2–3 Felder. Sie werden als Bänder untereinander auf einem
          Bildschirm angezeigt (Reihenfolge = Auswahl-Reihenfolge).
        </p>

        <div className="mt-3 flex max-h-72 flex-col gap-1 overflow-y-auto">
          {fields.map((f) => {
            const pos = selected.indexOf(f.id);
            const checked = pos >= 0;
            const atCap = !checked && selected.length >= MAX;
            return (
              <label
                key={f.id}
                className={`flex items-center gap-2.5 rounded-lg border px-3 py-2 text-sm ${
                  checked
                    ? "border-slate-400 bg-slate-50"
                    : "border-slate-200"
                } ${atCap ? "opacity-40" : "cursor-pointer"}`}
              >
                <input
                  type="checkbox"
                  checked={checked}
                  disabled={atCap}
                  onChange={() => toggle(f.id)}
                />
                <span className="flex-1 text-slate-700">
                  {f.label}
                  {f.location ? (
                    <span className="text-slate-400"> · {f.location}</span>
                  ) : null}
                </span>
                {checked && (
                  <span className="rounded bg-slate-700 px-1.5 text-xs font-bold text-white">
                    {pos + 1}
                  </span>
                )}
              </label>
            );
          })}
        </div>

        <div className="mt-4 flex items-center justify-between">
          <span className="text-xs text-slate-400">
            {selected.length} / {MAX} gewählt
          </span>
          <div className="flex gap-2">
            <button
              onClick={onCancel}
              className="rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium
                         text-slate-700 transition-colors hover:bg-slate-200"
            >
              Abbrechen
            </button>
            <button
              onClick={() => onConfirm(selected)}
              disabled={selected.length < 2}
              className="rounded-lg bg-slate-800 px-3.5 py-1.5 text-sm font-medium
                         text-white transition-colors hover:bg-slate-900
                         disabled:cursor-not-allowed disabled:opacity-40"
            >
              Übernehmen
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Eine Monitor-Einrichtungs-Adresse: Icon, Adresse, Kopier-Button und –
 * im Doppelmodus (`showBadge`) – ein „LAN"-/„Cloud"-Badge. Einzelmodus:
 * `showBadge=false`, die Zeile sieht aus wie zuvor.
 */
function MonitorAddressRow({
  url,
  kind,
  showBadge,
}: {
  url: string;
  kind: "lan" | "cloud";
  showBadge: boolean;
}) {
  const cloud = kind === "cloud";
  return (
    <div className="flex items-center gap-3 rounded-lg border border-slate-200 bg-white p-2.5 shadow-sm">
      <Tv size={18} className="shrink-0 text-slate-400" />
      {showBadge && (
        <span
          className={`inline-flex shrink-0 items-center gap-1 rounded-full px-2
                      py-0.5 text-[10px] font-medium ${
                        cloud
                          ? "bg-sky-100 text-sky-700"
                          : "bg-slate-200 text-slate-600"
                      }`}
        >
          {cloud ? <Cloud size={11} /> : <Wifi size={11} />}
          {cloud ? "Cloud" : "LAN"}
        </span>
      )}
      <code className="min-w-0 flex-1 truncate text-sm">{url}</code>
      <CopyButton url={url} />
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
