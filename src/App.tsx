import { useEffect, useState } from "react";
import { getStatus, loadConfig, saveConfig, startSync, stopSync } from "./api";
import { AlertBanner } from "./components/AlertBanner";
import { AppShell } from "./components/AppShell";
import { Footer } from "./components/Footer";
import { MatchAnnouncer } from "./components/MatchAnnouncer";
import type { NavView, SettingsFocus } from "./components/SideNav";
import { UpdateBanner, UpdateProvider } from "./components/UpdateBanner";
import { WalkoverPanel } from "./components/WalkoverPanel";
import { AnnouncePage } from "./pages/AnnouncePage";
import { CourtMonitorPanel } from "./pages/CourtMonitorPanel";
import { Dashboard } from "./pages/Dashboard";
import { FieldOverviewPage } from "./pages/FieldOverviewPage";
import { SetupWizard } from "./pages/SetupWizard";
import { TabletPanel } from "./pages/TabletPanel";
import type { AppConfig, SyncStatus } from "./types";

// "loading"/"wizard" sind Sonderzustände ohne Shell; alles andere sind die
// über die Seitenleiste erreichbaren Bereiche (NavView).
type View = "loading" | "wizard" | NavView;

function defaultConfig(): AppConfig {
  return {
    btp: { host: "127.0.0.1", port: 9901, password: null },
    badhub: {
      url: "https://badhub.de/api/live_update.php",
      password: "",
      live_url: "",
    },
    upload_logs: false,
    install_id: "",
    connection_mode: "lan",
    announce: {
      enabled: false,
      language_mode: "auto",
      voice_de: "",
      voice_en: "",
      rate: 0.8,
      gong: true,
    },
    court_monitor: {
      enabled: false,
      ad_interval_s: 10,
      show_discipline: true,
      show_round: true,
      show_match_number: true,
      show_timer: true,
      show_match_clock: true,
      show_ads: true,
      layout: "split",
    },
    call_timer: {
      enabled: false,
      second_call_minutes: 2,
      third_call_minutes: 4,
    },
    auto_assign: {
      enabled: false,
      wait_minutes: 1,
    },
    locked_courts: [],
  };
}

function App() {
  const [view, setView] = useState<View>("loading");
  const [config, setConfig] = useState<AppConfig>(defaultConfig());
  // Einstellungen-Abschnitt, zu dem beim Öffnen gesprungen wird (Klick auf
  // einen ausgegrauten Menüpunkt).
  const [settingsFocus, setSettingsFocus] = useState<SettingsFocus | undefined>();
  // Live-Status zentral – geteilt von Kopfzeile (Start/Stopp) und Status-Seite.
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    loadConfig()
      .then((c) => {
        // Installations-ID einmalig erzeugen – sie ordnet hochgeladene
        // Diagnose-Logs einer Installation zu.
        if (!c.install_id) {
          c = { ...c, install_id: crypto.randomUUID() };
          void saveConfig(c);
        }
        setConfig(c);
        // Ist bereits ein Badhub-Passwort hinterlegt, gilt die App als
        // eingerichtet und zeigt direkt das Dashboard.
        setView(c.badhub.password ? "dashboard" : "wizard");
      })
      .catch(() => setView("wizard"));
  }, []);

  // Status zentral pollen, sobald die App eingerichtet ist (nicht im
  // Wizard/Loading – dort läuft noch kein Sync).
  useEffect(() => {
    if (view === "loading" || view === "wizard") return;
    let active = true;
    const tick = () => {
      getStatus()
        .then((s) => {
          if (active) setStatus(s);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [view]);

  async function toggleRun() {
    if (!status) return;
    setBusy(true);
    try {
      if (status.running) {
        await stopSync();
      } else {
        await startSync();
      }
      setStatus(await getStatus());
    } finally {
      setBusy(false);
    }
  }

  // Navigation aus der Seitenleiste; bei einem ausgegrauten Punkt steht der
  // Ziel-Abschnitt der Einstellungen in `focus`.
  function navigate(next: NavView, focus?: SettingsFocus) {
    setSettingsFocus(next === "settings" ? focus : undefined);
    setView(next);
  }

  if (view === "loading") {
    return (
      <div className="flex h-full flex-col bg-slate-50">
        <main className="flex flex-1 items-center justify-center text-slate-400">
          Lädt …
        </main>
      </div>
    );
  }

  // Erst-Einrichtung: Wizard im Vollbild, ohne Shell.
  if (view === "wizard") {
    return (
      <UpdateProvider>
        <div className="flex h-full flex-col bg-slate-50">
          <UpdateBanner />
          <AlertBanner />
          <div className="min-h-0 flex-1 overflow-auto">
            <SetupWizard
              initialConfig={config}
              onDone={(c) => {
                setConfig(c);
                setView("dashboard");
              }}
            />
          </div>
          <Footer />
          <MatchAnnouncer announce={config.announce} />
        </div>
      </UpdateProvider>
    );
  }

  function activePage(v: NavView) {
    switch (v) {
      case "dashboard":
        return <Dashboard config={config} status={status} />;
      case "fields":
        return (
          <FieldOverviewPage
            callTimer={config.call_timer}
            announce={config.announce}
          />
        );
      case "tablets":
        return <TabletPanel announce={config.announce} />;
      case "announce":
        return (
          <AnnouncePage
            announce={config.announce}
            callTimer={config.call_timer}
          />
        );
      case "monitors":
        return <CourtMonitorPanel />;
      case "settings":
        // Hinweis: SetupWizard liest seine Felder einmalig aus initialConfig.
        // Das ist sicher, weil `config` nur beim Speichern (onDone) wechselt –
        // dann zeigt die Seite ohnehin den gespeicherten Stand. Würde `config`
        // künftig während offener Einstellungen von außen geändert, müsste die
        // Seite per key remountet werden.
        return (
          <SetupWizard
            mode="settings"
            focus={settingsFocus}
            initialConfig={config}
            onDone={(c) => setConfig(c)}
          />
        );
      default: {
        // Erzwingt zur Compile-Zeit, dass jeder NavView-Fall behandelt ist.
        const _exhaustive: never = v;
        return _exhaustive;
      }
    }
  }

  return (
    <UpdateProvider>
      <div className="flex h-full flex-col bg-slate-50">
        <UpdateBanner />
        <AlertBanner />
        <AppShell
          current={view}
          config={config}
          status={status}
          busy={busy}
          onToggleRun={toggleRun}
          onNavigate={navigate}
        >
          {activePage(view)}
        </AppShell>
        <Footer />
        <WalkoverPanel />
        <MatchAnnouncer announce={config.announce} />
      </div>
    </UpdateProvider>
  );
}

export default App;
