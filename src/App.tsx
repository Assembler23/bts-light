import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [version, setVersion] = useState("");

  useEffect(() => {
    invoke<string>("app_version").then(setVersion).catch(() => setVersion("?"));
  }, []);

  return (
    <main className="flex h-full flex-col items-center justify-center gap-3 bg-slate-50 text-slate-800">
      <h1 className="text-3xl font-semibold tracking-tight">BTS Light</h1>
      <p className="text-sm text-slate-500">
        Liveticker-Brücke zwischen BTP und badhub.de
      </p>
      <span className="rounded-full bg-slate-200 px-3 py-1 text-xs text-slate-600">
        v{version || "…"} · Phase 0 – Skeleton
      </span>
    </main>
  );
}

export default App;
