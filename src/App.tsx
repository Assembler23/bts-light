import { useEffect, useState } from "react";
import { loadConfig } from "./api";
import { UpdateBanner, UpdateProvider } from "./components/UpdateBanner";
import { Dashboard } from "./pages/Dashboard";
import { SetupWizard } from "./pages/SetupWizard";
import { TabletPanel } from "./pages/TabletPanel";
import type { AppConfig } from "./types";

type View = "loading" | "wizard" | "dashboard" | "tablets";

function defaultConfig(): AppConfig {
  return {
    btp: { host: "127.0.0.1", port: 9901, password: null },
    badhub: {
      url: "https://badhub.de/api/live_update.php",
      password: "",
      live_url: "",
    },
  };
}

function App() {
  const [view, setView] = useState<View>("loading");
  const [config, setConfig] = useState<AppConfig>(defaultConfig());

  useEffect(() => {
    loadConfig()
      .then((c) => {
        setConfig(c);
        // Ist bereits ein Badhub-Passwort hinterlegt, gilt die App als
        // eingerichtet und zeigt direkt das Dashboard.
        setView(c.badhub.password ? "dashboard" : "wizard");
      })
      .catch(() => setView("wizard"));
  }, []);

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
      return <TabletPanel onBack={() => setView("dashboard")} />;
    }
    return (
      <Dashboard
        config={config}
        onReconfigure={() => setView("wizard")}
        onOpenTablets={() => setView("tablets")}
      />
    );
  }

  return (
    <UpdateProvider>
      <div className="flex h-full flex-col">
        <UpdateBanner />
        <div className="min-h-0 flex-1 overflow-auto">{renderView()}</div>
      </div>
    </UpdateProvider>
  );
}

export default App;
