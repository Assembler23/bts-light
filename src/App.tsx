import { useEffect, useState } from "react";
import { loadConfig, saveConfig } from "./api";
import { AlertBanner } from "./components/AlertBanner";
import { Footer } from "./components/Footer";
import { MatchAnnouncer } from "./components/MatchAnnouncer";
import { UpdateBanner, UpdateProvider } from "./components/UpdateBanner";
import { WalkoverPanel } from "./components/WalkoverPanel";
import { CourtMonitorPanel } from "./pages/CourtMonitorPanel";
import { Dashboard } from "./pages/Dashboard";
import { FieldOverviewPage } from "./pages/FieldOverviewPage";
import { SetupWizard } from "./pages/SetupWizard";
import { TabletPanel } from "./pages/TabletPanel";
import type { AppConfig } from "./types";

type View = "loading" | "wizard" | "dashboard" | "tablets" | "monitors" | "fields";

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
    locked_courts: [],
  };
}

function App() {
  const [view, setView] = useState<View>("loading");
  const [config, setConfig] = useState<AppConfig>(defaultConfig());

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

  // Vor dem Öffnen des Wizards die Config frisch von der Platte laden – sonst
  // überschreibt buildConfig() Änderungen, die seit App-Start passiert sind
  // (z. B. in der Spielübersicht gesperrte Felder), mit dem veralteten Stand.
  function openWizard() {
    loadConfig()
      .then((c) => setConfig(c))
      .catch(() => {})
      .finally(() => setView("wizard"));
  }

  function renderView() {
    if (view === "loading") {
      return (
        <main className="flex h-full items-center justify-center text-slate-400">
          Lädt …
        </main>
      );
    }
    if (view === "wizard") {
      return (
        <SetupWizard
          initialConfig={config}
          onDone={(c) => {
            setConfig(c);
            setView("dashboard");
          }}
        />
      );
    }
    if (view === "tablets") {
      return (
        <TabletPanel
          onBack={() => setView("dashboard")}
          announce={config.announce}
        />
      );
    }
    if (view === "monitors") {
      return <CourtMonitorPanel onBack={() => setView("dashboard")} />;
    }
    if (view === "fields") {
      return <FieldOverviewPage onBack={() => setView("dashboard")} />;
    }
    return (
      <Dashboard
        config={config}
        onReconfigure={openWizard}
        onOpenTablets={() => setView("tablets")}
        onOpenMonitors={() => setView("monitors")}
        onOpenFields={() => setView("fields")}
      />
    );
  }

  return (
    <UpdateProvider>
      <div className="flex h-full flex-col bg-slate-50">
        <UpdateBanner />
        <AlertBanner />
        <div className="min-h-0 flex-1 overflow-auto">{renderView()}</div>
        <Footer />
        <WalkoverPanel />
        <MatchAnnouncer announce={config.announce} />
      </div>
    </UpdateProvider>
  );
}

export default App;
