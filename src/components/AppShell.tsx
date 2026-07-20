// App-Rahmen: persistente Kopfzeile (Verband, Live-Status, Start/Stopp) plus
// die Seitenleiste. Der gewählte Bereich wird als `children` im Inhaltsbereich
// gerendert – ohne Zurück-Button, denn die Navigation ist immer sichtbar.
import { type ReactNode } from "react";
import { Globe, Play, Square, Wifi, WifiOff } from "lucide-react";
import { tenantShortLabel } from "../presets";
import type {
  AppConfig,
  InternetStatus,
  SlaveInfo,
  SyncStatus,
  WifiStatus,
} from "../types";
import { type NavView, type SettingsFocus, SideNav } from "./SideNav";

function dotColor(status: SyncStatus | null): string {
  if (!status || !status.running) return "bg-slate-400";
  if (status.kind === "ok") return "bg-emerald-500";
  if (status.kind === "idle") return "bg-amber-400";
  // "warn" = selbstheilender Zwischenzustand (z. B. verworfener leerer
  // BTP-Snapshot) – bewusst KEIN Rot, das ist kein Ausfall.
  if (status.kind === "warn") return "bg-orange-400";
  return "bg-rose-500";
}

// Netzwerk-Anzeige für die Kopfzeile: zeigt, ob der PC im lokalen BTS-Netzwerk
// hängt (btsaccess-WLAN oder 192.168.16.x am LAN) – darüber erreichen ihn die
// LAN-Tablets/Pi-Monitore. Tablets im Cloud-Modus sind davon unabhängig.
// Grün = im BTS-Netz; sonst neutral mit dem aktuell verbundenen WLAN-Namen.
function NetworkIndicator({ wifi }: { wifi: WifiStatus | null }) {
  if (!wifi) return null; // noch kein Status (erster Poll läuft)
  if (wifi.bts_network) {
    return (
      <div
        className="flex items-center gap-1.5 text-emerald-600"
        title={
          wifi.ssid
            ? `Im BTS-Netzwerk (WLAN „${wifi.ssid}")`
            : "Im BTS-Netzwerk (LAN-Kabel, 192.168.16.x)"
        }
      >
        <Wifi size={15} />
        <span className="text-xs font-medium">BTS-Netzwerk</span>
      </div>
    );
  }
  // Nicht im BTS-Netz – kein Fehler (z. B. reiner Cloud-Betrieb), aber sichtbar.
  return (
    <div
      className="flex items-center gap-1.5 text-slate-400"
      title={
        wifi.ssid
          ? `Nicht im BTS-Netzwerk – aktuell WLAN „${wifi.ssid}" (erwartet btsaccess bzw. 192.168.16.x)`
          : "Nicht im BTS-Netzwerk (kein WLAN, kein BTS-LAN)"
      }
    >
      <WifiOff size={15} />
      <span className="max-w-[10rem] truncate text-xs">
        {wifi.ssid ? `Kein BTS-Netz (${wifi.ssid})` : "Kein BTS-Netz"}
      </span>
    </div>
  );
}

// Internet-/Uplink-Anzeige: ist die badhub-Cloud erreichbar (LTE/Internet
// aktiv)? Voraussetzung für Cloud-Logs + Liveticker-Push. Carriername (z. B.
// Vodafone) lässt sich vom PC aus nicht ermitteln → Label „Internet".
function InternetIndicator({ internet }: { internet: InternetStatus | null }) {
  if (!internet) return null; // erster Poll läuft noch
  if (internet.online) {
    return (
      <div
        className="flex items-center gap-1.5 text-emerald-600"
        title="Internet erreichbar (Uplink aktiv) – Cloud-Logs & Push gehen raus"
      >
        <Globe size={15} />
        <span className="text-xs font-medium">Internet</span>
      </div>
    );
  }
  return (
    <div
      className="flex items-center gap-1.5 text-rose-500"
      title="Kein Internet – badhub-Cloud nicht erreichbar (LTE-/Uplink prüfen). Lokaler Betrieb läuft weiter."
    >
      <Globe size={15} />
      <span className="text-xs font-medium">Kein Internet</span>
    </div>
  );
}

// Ferne-Hallen-Anzeige (Cloud-Master): ist der Ansage-Slave der anderen Halle
// online? Rein informativ; leer, wenn kein Slave bekannt ist (Einzelhalle).
function SlaveIndicator({ slaves }: { slaves: SlaveInfo[] }) {
  if (slaves.length === 0) return null;
  return (
    <div className="flex items-center gap-2.5">
      {slaves.map((s) => (
        <div
          key={s.id}
          className={`flex items-center gap-1 ${s.online ? "text-emerald-600" : "text-rose-500"}`}
          title={
            s.online
              ? `Ferne Halle „${s.hall || "?"}" verbunden (Cloud)`
              : `Ferne Halle „${s.hall || "?"}" offline – kein Internet? Ansage pausiert dort, bis sie wieder verbindet`
          }
        >
          <span
            className={`h-2 w-2 rounded-full ${s.online ? "bg-emerald-500" : "bg-rose-400"}`}
          />
          <span className="text-xs font-medium">{s.hall || "Ferne Halle"}</span>
        </div>
      ))}
    </div>
  );
}

export function AppShell({
  current,
  config,
  status,
  wifi,
  internet,
  slaves,
  busy,
  onToggleRun,
  onNavigate,
  children,
}: {
  current: NavView;
  config: AppConfig;
  status: SyncStatus | null;
  wifi: WifiStatus | null;
  internet: InternetStatus | null;
  slaves: SlaveInfo[];
  busy: boolean;
  onToggleRun: () => void;
  onNavigate: (view: NavView, focus?: SettingsFocus) => void;
  children: ReactNode;
}) {
  const running = status?.running ?? false;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Kopfzeile – auf jeder Seite gleich. */}
      <header className="flex shrink-0 items-center gap-3 border-b border-slate-200 bg-white px-4 py-2.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-slate-800 text-base">
          🏸
        </div>
        <div className="min-w-0">
          <div className="text-sm font-semibold leading-tight text-slate-800">
            BTS Light
          </div>
          <div className="truncate text-xs text-slate-500">
            {tenantShortLabel(config.badhub)}
          </div>
        </div>

        {/* Live-Status-Punkt mittig. */}
        <div className="ml-4 flex items-center gap-2">
          <span
            className={`h-2.5 w-2.5 rounded-full ${dotColor(status)} ${
              running && status?.kind === "ok" ? "animate-pulse" : ""
            }`}
          />
          <span className="text-sm text-slate-600">
            {running ? "Liveticker aktiv" : "Gestoppt"}
          </span>
        </div>

        {/* Netzwerk-Anzeige: hängt der PC im lokalen BTS-Netzwerk? */}
        <div className="ml-3 border-l border-slate-200 pl-3">
          <NetworkIndicator wifi={wifi} />
        </div>

        {/* Internet-/Uplink-Anzeige (LTE/Cloud erreichbar?). */}
        <div className="ml-3 border-l border-slate-200 pl-3">
          <InternetIndicator internet={internet} />
        </div>

        {/* Ferne Hallen (Cloud-Slaves) online? Nur sichtbar, wenn vorhanden. */}
        {slaves.length > 0 && (
          <div className="ml-3 border-l border-slate-200 pl-3">
            <SlaveIndicator slaves={slaves} />
          </div>
        )}

        {/* Start/Stopp – von überall erreichbar. */}
        <button
          onClick={onToggleRun}
          disabled={busy || !status}
          title={
            running
              ? "Liveticker und Tablet-Server anhalten"
              : "Liveticker starten – BTP wird verbunden"
          }
          className={`ml-auto inline-flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-sm
                      font-medium text-white transition-colors disabled:opacity-50 ${
                        running
                          ? "bg-rose-600 hover:bg-rose-700"
                          : "bg-emerald-600 hover:bg-emerald-700"
                      }`}
        >
          {running ? <Square size={15} /> : <Play size={15} />}
          {running ? "Stoppen" : "Starten"}
        </button>
      </header>

      {/* Seitenleiste + Inhalt. */}
      <div className="flex min-h-0 flex-1">
        <SideNav current={current} config={config} onNavigate={onNavigate} />
        <div className="min-h-0 flex-1 overflow-auto">{children}</div>
      </div>
    </div>
  );
}
