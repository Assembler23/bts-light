import { useEffect, useState } from "react";
import { loadConfig } from "./api";
import { Dashboard } from "./pages/Dashboard";
import { SetupWizard } from "./pages/SetupWizard";
import type { AppConfig } from "./types";

type View = "loading" | "wizard" | "dashboard";

function defaultConfig(): AppConfig {
  return {
    btp: { host: "127.0.0.1", port: 9901, password: null },
    badhub: { url: "https://badhub.de/api/live_update.php", password: "" },
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
        onDone={() => setView("dashboard")}
      />
    );
  }

  return <Dashboard onReconfigure={() => setView("wizard")} />;
}

export default App;
