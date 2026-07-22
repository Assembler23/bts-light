import { useEffect, useState } from "react";
import {
  cloudSlaves,
  fetchPronunciations,
  getStatus,
  internetStatus,
  loadConfig,
  saveConfig,
  startSync,
  stopSync,
  wifiStatus,
} from "./api";
import { setSharedOverrides } from "./io/announcer";
import { AlertBanner } from "./components/AlertBanner";
import { AzureFallbackBanner } from "./components/AzureFallbackBanner";
import { SlaveConnectBanner } from "./components/SlaveConnectBanner";
import { AppShell } from "./components/AppShell";
import { Footer } from "./components/Footer";
import { CloudAnnounceSlave } from "./components/CloudAnnounceSlave";
import { FreetextAnnouncer } from "./components/FreetextAnnouncer";
import { MatchAnnouncer } from "./components/MatchAnnouncer";
import type { NavView, SettingsFocus } from "./components/SideNav";
import { UpdateBanner, UpdateProvider } from "./components/UpdateBanner";
import { WalkoverPanel } from "./components/WalkoverPanel";
import { AnnouncePage } from "./pages/AnnouncePage";
import { CourtMonitorPanel } from "./pages/CourtMonitorPanel";
import { Dashboard } from "./pages/Dashboard";
import { FieldOverviewPage } from "./pages/FieldOverviewPage";
import { MaintenancePage } from "./pages/MaintenancePage";
import { SetupWizard } from "./pages/SetupWizard";
import { TabletPanel } from "./pages/TabletPanel";
import { WinnersPage } from "./pages/WinnersPage";
import type {
  AppConfig,
  InternetStatus,
  SlaveInfo,
  SyncStatus,
  WifiStatus,
} from "./types";

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
    slave_mode: false,
    master_namespace: "",
    announce: {
      enabled: false,
      language_mode: "auto",
      voice_de: "",
      voice_en: "",
      rate: 0.8,
      gong: true,
      name_overrides: [],
      name_overrides_enabled: true,
      announce_hall: "",
      saved_announcements: [],
      share_corrections: false,
    },
    azure_tts: {
      enabled: false,
      region: "",
      key: "",
      voice: "de-DE-SeraphinaMultilingualNeural",
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
      combo_vertical: false,
    },
    call_timer: {
      enabled: false,
      second_call_minutes: 2,
      third_call_minutes: 4,
    },
    scorekeeper: {
      enabled: false,
      break_seconds: 300,
    },
    auto_assign: {
      enabled: false,
      wait_minutes: 1,
      pause_minutes: 0,
      active_hall: "",
    },
    discipline_hall_rules: [],
    locked_courts: [],
    tablet_settings_pin: "0000",
    tournament_logo: { data: "", mime: "", background_color: "" },
  };
}

function App() {
  const [view, setView] = useState<View>("loading");
  const [config, setConfig] = useState<AppConfig>(defaultConfig());
  // Einstellungen-Abschnitt, zu dem beim Öffnen gesprungen wird (Klick auf
  // einen ausgegrauten Menüpunkt).
  const [settingsFocus, setSettingsFocus] = useState<
    SettingsFocus | undefined
  >();
  // Live-Status zentral – geteilt von Kopfzeile (Start/Stopp) und Status-Seite.
  const [status, setStatus] = useState<SyncStatus | null>(null);
  // WLAN des PCs für die Kopfzeile (zeigt, ob er im btsaccess-Netz hängt).
  const [wifi, setWifi] = useState<WifiStatus | null>(null);
  // Internet-/Uplink-Status (LTE/Cloud erreichbar?) für die Kopfzeile.
  const [internet, setInternet] = useState<InternetStatus | null>(null);
  // Ferne Hallen (Cloud-Slaves) für die Kopfzeilen-Anzeige am Master.
  const [slaves, setSlaves] = useState<SlaveInfo[]>([]);
  // Erst nach dem ersten echten cloudSlaves-Ergebnis wahr — Gate für den
  // SlaveConnectBanner, damit dessen Baseline nicht der leere Anfangszustand ist.
  const [slavesLoaded, setSlavesLoaded] = useState(false);
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

  // Geteiltes Aussprache-Wörterbuch laden: einmal beim Start (nach dem
  // Config-Load, damit die Badhub-URL steht) und danach alle 30 Min, solange
  // Internet da ist. Offline liefert der Rust-Cache den letzten Stand. Der
  // 30-Min-Takt (früher 3 h) sorgt zusammen mit dem 15-Min-Server-Job + dem
  // 5-Min-Edge-Cache dafür, dass Auto-Aussprachen eines laufenden Turniers
  // zeitnah ankommen (nicht erst Stunden später).
  useEffect(() => {
    let active = true;
    const refresh = () => {
      fetchPronunciations()
        .then((list) => {
          if (active) setSharedOverrides(list);
        })
        .catch(() => {});
    };
    // Kurzer Versatz, damit load_config den Rust-State zuerst gesetzt hat.
    // Danach eine Warm-up-Phase: der Turnier-Push stößt serverseitig die
    // Auto-Aussprache für die gerade geladenen Namen an (ADR 0008); die kurzen
    // Nachlade-Zeitpunkte holen das Ergebnis binnen weniger Minuten ans Feld,
    // statt bis zum nächsten 30-Min-Poll zu warten. Danach steady alle 30 Min.
    const warmupMs = [1500, 2 * 60 * 1000, 7 * 60 * 1000, 15 * 60 * 1000];
    const timers = warmupMs.map((ms) => window.setTimeout(refresh, ms));
    const id = window.setInterval(refresh, 30 * 60 * 1000);
    return () => {
      active = false;
      timers.forEach((t) => window.clearTimeout(t));
      window.clearInterval(id);
    };
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

  // WLAN seltener pollen (15 s) – die SSID wechselt selten, und jeder Aufruf
  // startet auf dem PC einen kurzen netsh-Prozess.
  useEffect(() => {
    if (view === "loading" || view === "wizard") return;
    let active = true;
    // Überlappende Aufrufe vermeiden: hängt das WLAN-Tool ausnahmsweise, darf
    // der 15-s-Tick keine weiteren Aufrufe nachschieben.
    let inflight = false;
    const tick = () => {
      if (inflight) return;
      inflight = true;
      wifiStatus()
        .then((w) => {
          if (active) setWifi(w);
        })
        .catch(() => {})
        .finally(() => {
          inflight = false;
        });
    };
    tick();
    const id = setInterval(tick, 15000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [view]);

  // Internet/Uplink alle 30 s prüfen (HEAD auf badhub.de) – seltener, weil es
  // einen echten Netz-Roundtrip macht und über LTE Daten kostet.
  useEffect(() => {
    if (view === "loading" || view === "wizard") return;
    let active = true;
    let inflight = false;
    const tick = () => {
      if (inflight) return;
      inflight = true;
      internetStatus()
        .then((s) => {
          if (active) setInternet(s);
        })
        .catch(() => {})
        .finally(() => {
          inflight = false;
        });
    };
    tick();
    const id = setInterval(tick, 30000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [view]);

  // Ferne Hallen (Cloud-Slaves) alle 5 s prüfen – für die Kopfzeilen-Anzeige.
  // Der Command liefert leer, wenn dieser PC kein Cloud-Master ist (kein
  // Aufwand bei Einzelhallen).
  useEffect(() => {
    if (view === "loading" || view === "wizard") return;
    let active = true;
    const tick = () => {
      cloudSlaves()
        .then((s) => {
          if (!active) return;
          setSlaves(s);
          // Erst nach dem ersten ECHTEN Poll-Ergebnis darf der
          // SlaveConnectBanner seine Baseline ziehen — sonst gälte der
          // leere Anfangszustand als Baseline und bereits verbundene
          // Hallen würden beim App-Start fälschlich als „neu verbunden"
          // gemeldet (Review-Befund).
          setSlavesLoaded(true);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 5000);
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
          <AzureFallbackBanner />
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
          <MatchAnnouncer
            announce={config.announce}
            azureTts={config.azure_tts}
          />
        </div>
      </UpdateProvider>
    );
  }

  function activePage(v: NavView) {
    switch (v) {
      case "dashboard":
        return (
          <Dashboard
            config={config}
            status={status}
            onNavigate={navigate}
            onConfigSaved={(c) => setConfig(c)}
          />
        );
      case "fields":
        return (
          <FieldOverviewPage
            callTimer={config.call_timer}
            announce={config.announce}
            azureTts={config.azure_tts}
            disciplineHallRules={config.discipline_hall_rules}
            manageScorekeepers={config.scorekeeper?.enabled ?? false}
          />
        );
      case "tablets":
        return (
          <TabletPanel announce={config.announce} azureTts={config.azure_tts} />
        );
      case "announce":
        return (
          <AnnouncePage
            announce={config.announce}
            callTimer={config.call_timer}
            azureTts={config.azure_tts}
            config={config}
            onConfigSaved={(c) => setConfig(c)}
          />
        );
      case "monitors":
        return <CourtMonitorPanel config={config} />;
      case "winners":
        return <WinnersPage />;
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
      case "maintenance":
        return <MaintenancePage />;
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
        <AzureFallbackBanner />
        {slavesLoaded && <SlaveConnectBanner slaves={slaves} />}
        <AppShell
          current={view}
          config={config}
          status={status}
          wifi={wifi}
          internet={internet}
          slaves={slaves}
          busy={busy}
          onToggleRun={toggleRun}
          onNavigate={navigate}
        >
          {activePage(view)}
        </AppShell>
        <Footer />
        <WalkoverPanel />
        <MatchAnnouncer
          announce={config.announce}
          azureTts={config.azure_tts}
        />
        <FreetextAnnouncer
          announce={config.announce}
          azureTts={config.azure_tts}
        />
        <CloudAnnounceSlave
          announce={config.announce}
          azureTts={config.azure_tts}
        />
      </div>
    </UpdateProvider>
  );
}

export default App;
