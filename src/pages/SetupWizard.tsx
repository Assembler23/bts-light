import { type ReactNode, useEffect, useState } from "react";
import {
  Check,
  Cloud,
  Image,
  Info,
  KeyRound,
  LayoutGrid,
  type LucideIcon,
  Monitor,
  Server,
  Stethoscope,
  Target,
  Timer,
  Trash2,
  Volume2,
  Wifi,
  X,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  addCourtAd,
  listCourtAds,
  readTournamentLogo,
  removeCourtAd,
  setCourtAdLabel,
  saveConfig,
  startSync,
  stopSync,
  tabletOverview,
  testBtp,
} from "../api";
import { CopyBadgeButton } from "../components/CopyBadgeButton";
import { MonitorPreview } from "../components/MonitorPreview";
import { playNameTest, playTestAnnouncement } from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
import { BASE_NAME_OVERRIDES } from "../io/nameOverrideBase";
import { PRESETS, findPreset } from "../presets";
import { useAvailableVoices, voicesForLang } from "../state/useAvailableVoices";
import type {
  AnnounceLanguageMode,
  AppConfig,
  ConnectionMode,
  CourtAd,
  NameOverride,
} from "../types";

interface Props {
  initialConfig: AppConfig;
  onDone: (config: AppConfig) => void;
  /** "wizard" = Erst-Einrichtung (Vollbild, „Speichern & starten").
   *  "settings" = jederzeit erreichbare Einstellungen-Seite. */
  mode?: "wizard" | "settings";
  /** Abschnitt, zu dem beim Öffnen gescrollt wird (Sprung aus einem
   *  ausgegrauten Menüpunkt der Seitenleiste). */
  focus?: "ansagen" | "court-monitor";
}

type TestState =
  | { kind: "idle" }
  | { kind: "testing" }
  | { kind: "ok"; tournament: string }
  | { kind: "error"; message: string };

const MANUAL = "manual";

/** Abschnitts-Überschrift mit Icon. */
function SectionHeader({
  icon: Icon,
  children,
}: {
  icon: LucideIcon;
  children: ReactNode;
}) {
  return (
    <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-700">
      <Icon size={16} className="text-slate-400" />
      {children}
    </h2>
  );
}

/** Eine Auswahlkachel mit Icon, Titel, Beschreibung und Aktiv-Markierung. */
function ChoiceCard(props: {
  icon: LucideIcon;
  title: string;
  description: string;
  active: boolean;
  onClick: () => void;
}) {
  const Icon = props.icon;
  return (
    <button
      onClick={props.onClick}
      className={`flex w-full items-start gap-3 rounded-xl border px-4 py-3 text-left
                  transition-colors ${
                    props.active
                      ? "border-slate-800 bg-white shadow-sm"
                      : "border-slate-300 bg-white hover:border-slate-400"
                  }`}
    >
      <span
        className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center
                    rounded-lg ${
                      props.active
                        ? "bg-slate-800 text-white"
                        : "bg-slate-100 text-slate-500"
                    }`}
      >
        <Icon size={16} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{props.title}</span>
        <span className="block text-xs text-slate-500">
          {props.description}
        </span>
      </span>
      {props.active && (
        <Check size={16} className="mt-1 shrink-0 text-slate-800" />
      )}
    </button>
  );
}

/**
 * Eine unabhängig an-/abschaltbare Kachel (Icon, Titel, Beschreibung) –
 * für Verbindungswege, die sich kombinieren lassen. Anders als
 * `ChoiceCard` ist sie ein echter Schalter (eine Checkbox), nicht Teil
 * einer Einfachauswahl.
 */
function ToggleCard(props: {
  icon: LucideIcon;
  title: string;
  description: string;
  active: boolean;
  onToggle: () => void;
}) {
  const Icon = props.icon;
  return (
    <button
      onClick={props.onToggle}
      role="switch"
      aria-checked={props.active}
      className={`flex items-start gap-3 rounded-xl border px-4 py-3 text-left
                  transition-colors ${
                    props.active
                      ? "border-slate-800 bg-white shadow-sm"
                      : "border-slate-300 bg-white hover:border-slate-400"
                  }`}
    >
      <span
        className={`mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center
                    rounded-lg ${
                      props.active
                        ? "bg-slate-800 text-white"
                        : "bg-slate-100 text-slate-500"
                    }`}
      >
        <Icon size={16} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{props.title}</span>
        <span className="block text-xs text-slate-500">
          {props.description}
        </span>
      </span>
      <span
        className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center
                    rounded border ${
                      props.active
                        ? "border-slate-800 bg-slate-800 text-white"
                        : "border-slate-300 bg-white"
                    }`}
      >
        {props.active && <Check size={14} />}
      </span>
    </button>
  );
}

/** Ein beschriftetes Eingabefeld. */
function Field(props: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-sm font-medium text-slate-600">
        {props.label}
      </span>
      <input
        type={props.type ?? "text"}
        value={props.value}
        placeholder={props.placeholder}
        onChange={(e) => props.onChange(e.currentTarget.value)}
        className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                   focus:border-slate-500 focus:outline-none"
      />
    </label>
  );
}

export function SetupWizard({
  initialConfig,
  onDone,
  mode = "wizard",
  focus,
}: Props) {
  const isSettings = mode === "settings";
  // Vorauswahl des Verbands aus der gespeicherten Config ableiten – damit die
  // Einstellungen-Seite das tatsächlich aktive Ziel zeigt (nicht stur BVBB).
  const initialPreset = PRESETS.find(
    (p) =>
      (initialConfig.badhub.live_url &&
        p.badhub.live_url === initialConfig.badhub.live_url) ||
      (initialConfig.badhub.password &&
        p.badhub.password === initialConfig.badhub.password),
  );
  const [presetId, setPresetId] = useState(
    initialPreset?.id ?? (initialConfig.badhub.password ? MANUAL : "bvbb"),
  );
  const [host, setHost] = useState(initialConfig.btp.host);
  const [port, setPort] = useState(String(initialConfig.btp.port));
  const [btpPassword, setBtpPassword] = useState(initialConfig.btp.password ?? "");
  const [badhubUrl, setBadhubUrl] = useState(initialConfig.badhub.url);
  const [badhubPassword, setBadhubPassword] = useState(initialConfig.badhub.password);
  const [badhubLiveUrl, setBadhubLiveUrl] = useState(initialConfig.badhub.live_url);
  // Turnierlogo (badhub-Liveticker). BTP liefert keins → Upload.
  const [logoData, setLogoData] = useState(initialConfig.tournament_logo?.data ?? "");
  const [logoMime, setLogoMime] = useState(initialConfig.tournament_logo?.mime ?? "");
  const [logoBg, setLogoBg] = useState(
    initialConfig.tournament_logo?.background_color ?? "",
  );
  const [logoError, setLogoError] = useState("");
  const [uploadLogs, setUploadLogs] = useState(initialConfig.upload_logs);
  const [tabletPin, setTabletPin] = useState(
    initialConfig.tablet_settings_pin ?? "0000",
  );
  // LAN und Cloud sind unabhängig schaltbar – aus dem gespeicherten
  // connection_mode abgeleitet ("lan+cloud" → beide an).
  const [lanEnabled, setLanEnabled] = useState(
    initialConfig.connection_mode !== "cloud",
  );
  const [cloudEnabled, setCloudEnabled] = useState(
    initialConfig.connection_mode !== "lan",
  );
  const [annEnabled, setAnnEnabled] = useState(initialConfig.announce.enabled);
  const [annLang, setAnnLang] = useState<AnnounceLanguageMode>(
    initialConfig.announce.language_mode,
  );
  const [annVoiceDe, setAnnVoiceDe] = useState(initialConfig.announce.voice_de);
  const [annVoiceEn, setAnnVoiceEn] = useState(initialConfig.announce.voice_en);
  const [annRate, setAnnRate] = useState(initialConfig.announce.rate);
  const [annGong, setAnnGong] = useState(initialConfig.announce.gong);
  // Phonetische Aussprache-Korrekturen (Name/Namensteil → gesprochene Form),
  // z. B. für asiatische Namen, die die de/en-Stimme falsch ausspricht.
  const [annNameOverrides, setAnnNameOverrides] = useState<NameOverride[]>(
    initialConfig.announce.name_overrides ?? [],
  );
  const [annOverridesEnabled, setAnnOverridesEnabled] = useState(
    initialConfig.announce.name_overrides_enabled ?? true,
  );
  // Mehr-Hallen: diese Instanz sagt nur Spiele dieser Halle an (leer = alle).
  const [annHall, setAnnHall] = useState(
    initialConfig.announce.announce_hall ?? "",
  );
  // Azure Neural TTS (hochwertige Cloud-Ansage, opt-in).
  const az = initialConfig.azure_tts;
  const [azEnabled, setAzEnabled] = useState(az?.enabled ?? false);
  const [azRegion, setAzRegion] = useState(az?.region ?? "");
  const [azKey, setAzKey] = useState(az?.key ?? "");
  const [azVoice, setAzVoice] = useState(
    az?.voice || "de-DE-SeraphinaMultilingualNeural",
  );
  // Aufruf-Timer (1./2./3. Aufruf) – Schwellen in Minuten.
  const ct = initialConfig.call_timer;
  const [ctEnabled, setCtEnabled] = useState(ct?.enabled ?? false);
  const [ctSecond, setCtSecond] = useState(String(ct?.second_call_minutes ?? 2));
  const [ctThird, setCtThird] = useState(String(ct?.third_call_minutes ?? 4));
  // Automatische Feldvergabe.
  const aa = initialConfig.auto_assign;
  const [aaEnabled, setAaEnabled] = useState(aa?.enabled ?? false);
  const [aaWait, setAaWait] = useState(String(aa?.wait_minutes ?? 1));
  const [aaPause, setAaPause] = useState(String(aa?.pause_minutes ?? 0));
  const [aaActiveHall, setAaActiveHall] = useState(aa?.active_hall ?? "");
  const cm = initialConfig.court_monitor;
  const [cmEnabled, setCmEnabled] = useState(cm.enabled);
  const [cmInterval, setCmInterval] = useState(cm.ad_interval_s);
  const [cmDiscipline, setCmDiscipline] = useState(cm.show_discipline);
  const [cmRound, setCmRound] = useState(cm.show_round);
  const [cmMatchNumber, setCmMatchNumber] = useState(cm.show_match_number);
  const [cmTimer, setCmTimer] = useState(cm.show_timer);
  const [cmMatchClock, setCmMatchClock] = useState(cm.show_match_clock);
  const [cmAds, setCmAds] = useState(cm.show_ads);
  const [cmLayout, setCmLayout] = useState(cm.layout || "split");
  const [cmComboVertical, setCmComboVertical] = useState(cm.combo_vertical ?? false);
  const [ads, setAds] = useState<CourtAd[]>([]);
  const [adError, setAdError] = useState("");
  const voices = useAvailableVoices();
  const [test, setTest] = useState<TestState>({ kind: "idle" });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [saved, setSaved] = useState(false);

  // Hinterlegte Werbebilder beim Öffnen laden.
  useEffect(() => {
    listCourtAds()
      .then(setAds)
      .catch(() => {});
  }, []);

  // Sprung aus einem ausgegrauten Menüpunkt: zum passenden Abschnitt scrollen.
  useEffect(() => {
    if (!focus) return;
    const el = document.getElementById(`section-${focus}`);
    if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [focus]);

  // Hallen des laufenden Turniers (für die „Ansagen nur für Halle X"-Auswahl bei
  // Mehr-Hallen-Turnieren). Aus der Felder-Übersicht abgeleitet; leer, solange
  // keine Turnierdatei verbunden ist.
  const [halls, setHalls] = useState<string[]>([]);
  useEffect(() => {
    let active = true;
    tabletOverview()
      .then((info) => {
        if (!active) return;
        setHalls(
          [
            ...new Set(
              (info.courts ?? []).map((c) => c.location).filter((l) => l !== ""),
            ),
          ].sort((a, b) => a.localeCompare(b, "de")),
        );
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  const isManual = presetId === MANUAL;

  // Die beiden Modus-Schalter zurück auf einen connection_mode abbilden.
  const connectionMode: ConnectionMode =
    lanEnabled && cloudEnabled ? "lan+cloud" : cloudEnabled ? "cloud" : "lan";

  function buildConfig(): AppConfig {
    const preset = findPreset(presetId);
    const badhub =
      isManual || !preset
        ? {
            url: badhubUrl.trim(),
            password: badhubPassword.trim(),
            live_url: badhubLiveUrl.trim(),
          }
        : preset.badhub;
    return {
      btp: {
        host: host.trim(),
        port: Number(port) || 9901,
        password: btpPassword.trim() ? btpPassword.trim() : null,
      },
      badhub,
      upload_logs: uploadLogs,
      install_id: initialConfig.install_id,
      connection_mode: connectionMode,
      announce: {
        enabled: annEnabled,
        language_mode: annLang,
        voice_de: annVoiceDe,
        voice_en: annVoiceEn,
        rate: annRate,
        gong: annGong,
        name_overrides: annNameOverrides
          // Leere Zeilen (weder Name noch Aussprache) beim Speichern verwerfen.
          .map((o) => ({ name: o.name.trim(), say: o.say.trim() }))
          .filter((o) => o.name && o.say),
        name_overrides_enabled: annOverridesEnabled,
        announce_hall: annHall.trim(),
      },
      azure_tts: {
        enabled: azEnabled,
        region: azRegion.trim(),
        key: azKey.trim(),
        voice: azVoice,
      },
      court_monitor: {
        enabled: cmEnabled,
        ad_interval_s: cmInterval,
        show_discipline: cmDiscipline,
        show_round: cmRound,
        show_match_number: cmMatchNumber,
        show_timer: cmTimer,
        show_match_clock: cmMatchClock,
        show_ads: cmAds,
        layout: cmLayout,
        combo_vertical: cmComboVertical,
      },
      // Schwellen robust auflösen: ungültige/leere Eingabe → Standard; und der
      // 3. Aufruf muss nach dem 2. liegen (sonst übersprünge die Anzeige den
      // 2. Aufruf). Bei Fehlkonfig wird der 3. auf 2. + 1 Min angehoben.
      call_timer: (() => {
        const second = Number(ctSecond) > 0 ? Number(ctSecond) : 2;
        const thirdRaw = Number(ctThird) > 0 ? Number(ctThird) : 4;
        return {
          enabled: ctEnabled,
          second_call_minutes: second,
          third_call_minutes: thirdRaw > second ? thirdRaw : second + 1,
        };
      })(),
      auto_assign: {
        enabled: aaEnabled,
        // Negative/leere Eingabe abfangen; 0 ist erlaubt (sofort belegen).
        wait_minutes: Number(aaWait) >= 0 ? Number(aaWait) : 1,
        // Spieler-Pause nach Spielende; 0 = aus BTP (Setting 1303).
        pause_minutes: Number(aaPause) >= 0 ? Number(aaPause) : 0,
        // Aktive Halle (Tages-Halle) für Mehr-Hallen-Turniere; leer = alle.
        active_hall: aaActiveHall.trim(),
      },
      // Sperrliste unverändert durchreichen – wird im Wizard nicht editiert.
      locked_courts: initialConfig.locked_courts ?? [],
      // Tablet-Einstellungs-PIN: nur Ziffern, leer → Default „0000".
      tablet_settings_pin: tabletPin.replace(/\D/g, "").slice(0, 8) || "0000",
      tournament_logo: {
        data: logoData,
        mime: logoMime,
        background_color: logoBg.trim(),
      },
    };
  }

  /** Lässt den Nutzer ein Turnierlogo wählen und übernimmt es (Base64) sofort. */
  async function pickLogo() {
    setLogoError("");
    try {
      const sel = await open({
        multiple: false,
        filters: [
          { name: "Bilder", extensions: ["png", "jpg", "jpeg", "webp", "gif", "svg"] },
        ],
      });
      if (!sel) return;
      const path = Array.isArray(sel) ? sel[0] : sel;
      const { data, mime } = await readTournamentLogo(path);
      setLogoData(data);
      setLogoMime(mime);
    } catch (e) {
      setLogoError(String(e));
    }
  }

  /** Entfernt das hinterlegte Turnierlogo. */
  function clearLogo() {
    setLogoData("");
    setLogoMime("");
    setLogoError("");
  }

  /** Lässt den Nutzer Werbebilder wählen und übernimmt sie sofort. */
  async function pickAds() {
    setAdError("");
    try {
      const sel = await open({
        multiple: true,
        filters: [
          { name: "Bilder", extensions: ["jpg", "jpeg", "png", "webp", "gif"] },
        ],
      });
      if (!sel) return;
      const paths = Array.isArray(sel) ? sel : [sel];
      for (const p of paths) {
        await addCourtAd(p);
      }
      setAds(await listCourtAds());
    } catch (e) {
      setAdError(String(e));
    }
  }

  /** Entfernt ein hinterlegtes Werbebild. */
  async function deleteAd(file: string) {
    setAdError("");
    try {
      await removeCourtAd(file);
      setAds(await listCourtAds());
    } catch (e) {
      setAdError(String(e));
    }
  }

  const canSave =
    host.trim() !== "" &&
    (!isManual || (badhubUrl.trim() !== "" && badhubPassword.trim() !== "")) &&
    // Mindestens ein Tablet-Verbindungsweg muss aktiv sein.
    (lanEnabled || cloudEnabled);

  async function runTest() {
    setTest({ kind: "testing" });
    try {
      const name = await testBtp(host.trim(), Number(port) || 9901, btpPassword.trim() || null);
      setTest({ kind: "ok", tournament: name });
    } catch (e) {
      setTest({ kind: "error", message: String(e) });
    }
  }

  async function saveAndStart() {
    setSaving(true);
    setSaveError("");
    setSaved(false);
    try {
      const config = buildConfig();
      await saveConfig(config);
      // Sync sauber neu starten, damit ein geänderter Modus (LAN/Cloud)
      // sicher übernommen wird – ein laufender Sync würde sonst weiterlaufen.
      // Kurze Pause, damit der alte Tablet-Server den Port freigibt, bevor
      // der neue ihn bindet.
      await stopSync();
      await new Promise((r) => setTimeout(r, 400));
      await startSync();
      onDone(config);
      // In den Einstellungen bleibt die Seite stehen (Navigation via
      // Seitenleiste) – kurze Bestätigung statt Wechsel ins Dashboard.
      // Die Bestätigung nach 3 s ausblenden, damit ein später dazukommender
      // Helfer keinen veralteten „Gespeichert"-Hinweis fehldeutet.
      if (isSettings) {
        setSaved(true);
        setSaving(false);
        window.setTimeout(() => setSaved(false), 3000);
      }
    } catch (e) {
      setSaveError(String(e));
      setSaving(false);
    }
  }

  return (
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-6 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-slate-800 text-lg">
          🏸
        </div>
        <div>
          <h1 className="text-2xl font-semibold leading-tight">
            {isSettings ? "Einstellungen" : "BTS Light einrichten"}
          </h1>
          <p className="text-sm text-slate-500">
            {isSettings
              ? "Verband, BTP-Verbindung, Tablets, Ansagen und Monitore anpassen."
              : "Verbinde dein Turnier (BTP) mit dem Badhub-Liveticker."}
          </p>
        </div>
      </header>

      {/* Schritt 1: Verband / Ziel */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={Target}>1 · Liveticker-Ziel</SectionHeader>
        {PRESETS.map((preset) => (
          <div key={preset.id} className="flex items-stretch gap-2">
            <div className="min-w-0 flex-1">
              <ChoiceCard
                icon={Target}
                title={preset.label}
                description={preset.badhub.live_url}
                active={presetId === preset.id}
                onClick={() => setPresetId(preset.id)}
              />
            </div>
            <CopyBadgeButton liveUrl={preset.badhub.live_url} />
          </div>
        ))}
        <ChoiceCard
          icon={KeyRound}
          title="Anderes Turnier (manuell)"
          description="Badhub-URL und Passwort selbst eintragen"
          active={isManual}
          onClick={() => setPresetId(MANUAL)}
        />
      </section>

      {/* Turnierlogo (badhub-Liveticker) */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={Image}>Turnierlogo (optional)</SectionHeader>
        <p className="text-sm text-slate-600">
          Erscheint oben auf der Live-Seite (badhub.de/live). BTP liefert kein
          Logo – lade es hier hoch (PNG, JPG, WEBP, GIF, SVG; max. 2 MB).
        </p>
        <div className="flex items-center gap-3">
          {logoData ? (
            <img
              src={`data:${logoMime};base64,${logoData}`}
              alt="Turnierlogo-Vorschau"
              className="h-16 w-16 rounded-lg border border-slate-200 object-contain"
              style={{ background: logoBg || "#ffffff" }}
            />
          ) : (
            <div
              className="flex h-16 w-16 items-center justify-center rounded-lg
                         border border-dashed border-slate-300 text-slate-400"
            >
              <Image size={22} />
            </div>
          )}
          <div className="flex flex-col gap-1.5">
            <button
              onClick={() => void pickLogo()}
              className="self-start rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm
                         font-medium text-slate-700 transition-colors hover:bg-slate-200"
            >
              {logoData ? "Logo ersetzen" : "Logo wählen"}
            </button>
            {logoData && (
              <button
                onClick={clearLogo}
                className="inline-flex items-center gap-1 self-start text-xs
                           text-rose-700 hover:underline"
              >
                <Trash2 size={13} /> Entfernen
              </button>
            )}
          </div>
        </div>
        {logoData && (
          <label className="flex items-center gap-2 text-sm text-slate-700">
            Hintergrundfarbe
            <input
              type="color"
              value={logoBg || "#ffffff"}
              onChange={(e) => setLogoBg(e.target.value)}
              className="h-7 w-10 cursor-pointer rounded border border-slate-200"
            />
            <span className="text-xs text-slate-500">
              für transparente Logos (sonst Standard-Weiß)
            </span>
          </label>
        )}
        {logoError && <p className="text-xs text-rose-700">{logoError}</p>}
      </section>

      {/* Schritt 2: BTP-Verbindung */}
      <section className="flex flex-col gap-3">
        <SectionHeader icon={Server}>2 · BTP-Verbindung</SectionHeader>
        <Field label="BTP-Adresse" value={host} onChange={setHost} placeholder="127.0.0.1" />
        <Field label="Port" value={port} onChange={setPort} type="number" />
        <Field
          label="BTP-Passwort (falls gesetzt)"
          value={btpPassword}
          onChange={setBtpPassword}
          type="password"
        />
        <button
          onClick={runTest}
          disabled={test.kind === "testing" || host.trim() === ""}
          className="self-start rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium
                     text-slate-700 transition-colors hover:bg-slate-200 disabled:opacity-50"
        >
          {test.kind === "testing" ? "Teste …" : "Verbindung testen"}
        </button>
        {test.kind === "ok" && (
          <p className="flex items-center gap-1.5 text-sm text-emerald-700">
            <Check size={16} /> BTP gefunden – Turnier „{test.tournament}"
          </p>
        )}
        {test.kind === "error" && (
          <p className="flex items-start gap-1.5 text-sm text-rose-700">
            <X size={16} className="mt-0.5 shrink-0" /> {test.message}
          </p>
        )}
      </section>

      {/* Schritt 3: Badhub (nur manuell) */}
      {isManual && (
        <section className="flex flex-col gap-3">
          <SectionHeader icon={KeyRound}>3 · Badhub-Zugang</SectionHeader>
          <div className="flex gap-2.5 rounded-xl border border-sky-200 bg-sky-50 p-3.5 text-sm text-sky-900">
            <Info size={18} strokeWidth={2} className="mt-0.5 shrink-0 text-sky-600" />
            <p>
              Für ein <strong>eigenes Turnier</strong> brauchst du einen eigenen
              Zugang. Wende dich vorab an{" "}
              <a
                href="mailto:info@badhub.de"
                className="font-medium underline underline-offset-2"
              >
                info@badhub.de
              </a>{" "}
              — dann bekommst du eine individuelle Liveticker-Adresse und die
              passenden Zugangsdaten (URL&nbsp;+&nbsp;Passwort), die du hier
              einträgst.
            </p>
          </div>
          <Field label="Badhub-URL" value={badhubUrl} onChange={setBadhubUrl} />
          <Field
            label="Badhub-Passwort"
            value={badhubPassword}
            onChange={setBadhubPassword}
            type="password"
          />
          <Field
            label="Live-Seite (URL, optional)"
            value={badhubLiveUrl}
            onChange={setBadhubLiveUrl}
            placeholder="https://badhub.de/live?t=…"
          />
        </section>
      )}

      {/* Tablet-Verbindung */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={Wifi}>Tablet-Verbindung</SectionHeader>
        <p className="text-xs text-slate-500">
          Wie erreichen die Schiedsrichter-Tablets diesen PC? Beide Wege
          lassen sich zusammen aktivieren – etwa für ein Zwei-Hallen-Turnier
          (eine Halle per LAN, die andere über die Cloud). Lässt sich später
          in den Einstellungen umstellen.
        </p>
        <ToggleCard
          icon={Wifi}
          title="LAN – lokales Netz"
          description="Tablets verbinden sich direkt im Hallen-WLAN. Schnell und offline – braucht aber einen freigegebenen Port (Windows-Firewall)."
          active={lanEnabled}
          onToggle={() => setLanEnabled((v) => !v)}
        />
        <ToggleCard
          icon={Cloud}
          title="Über badhub.de – Cloud"
          description="Tablets und PC verbinden sich nur nach außen. Funktioniert auch hinter gesperrten Firmen-Firewalls – Internet vorausgesetzt."
          active={cloudEnabled}
          onToggle={() => setCloudEnabled((v) => !v)}
        />
        {!lanEnabled && !cloudEnabled && (
          <p className="flex items-start gap-1.5 text-xs text-rose-700">
            <X size={14} className="mt-0.5 shrink-0" />
            Mindestens einen Verbindungsweg aktivieren.
          </p>
        )}
        <div className="mt-1">
          <Field
            label="Tablet-Einstellungs-PIN"
            value={tabletPin}
            onChange={(v) => setTabletPin(v.replace(/\D/g, "").slice(0, 8))}
            placeholder="0000"
            type="tel"
          />
          <p className="mt-1 text-xs text-slate-500">
            Schützt das Zahnrad-Menü am Zähltablett (Feld wechseln ohne QR).
            Nur Ziffern. Reiner Bedien-Schutz – die echte Kiosk-Sperre macht
            der Kiosk-Browser (eigener Exit-PIN).
          </p>
        </div>
      </section>

      {/* Sprachansagen */}
      <section id="section-ansagen" className="flex flex-col gap-2 scroll-mt-4">
        <SectionHeader icon={Volume2}>Sprachansagen</SectionHeader>
        <p className="text-xs text-slate-500">
          Sagt jedes Spiel an, das in BTP auf ein Feld gezogen wird – mit
          Gong, Feldnummer, Disziplin und Paarung.
        </p>
        <label className="flex items-center gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={annEnabled}
            onChange={(e) => setAnnEnabled(e.currentTarget.checked)}
          />
          Sprachansagen aktivieren
        </label>

        {annEnabled && (
          <div className="mt-1 flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4">
            {/* Mehr-Hallen: nur eine Halle ansagen (jede Halle hört nur ihre
                eigenen Ansagen). Nur sichtbar, wenn ≥2 Hallen erkannt wurden
                oder bereits eine Halle eingestellt ist. */}
            {(halls.length >= 2 || annHall !== "") && (
              <label className="block rounded-lg bg-amber-50 p-3">
                <span className="mb-1 block text-sm font-medium text-amber-900">
                  Ansagen nur für Halle (Mehr-Hallen-Turnier)
                </span>
                <select
                  value={annHall}
                  onChange={(e) => setAnnHall(e.currentTarget.value)}
                  className="w-full rounded-lg border border-amber-300 bg-white px-3 py-2 text-sm
                             focus:border-amber-500 focus:outline-none"
                >
                  <option value="">Alle Hallen</option>
                  {(annHall !== "" && !halls.includes(annHall)
                    ? [...halls, annHall]
                    : halls
                  ).map((h) => (
                    <option key={h} value={h}>
                      {h}
                    </option>
                  ))}
                </select>
                <p className="mt-1 text-xs text-amber-700">
                  Dieser PC sagt dann nur Spiele dieser Halle an. „Alle Hallen" =
                  keine Einschränkung. So hört in einem 2-Hallen-Setup jede Halle
                  nur ihre eigenen Ansagen.
                </p>
              </label>
            )}

            {/* Sprache */}
            <div className="flex flex-col gap-1.5">
              <span className="text-sm font-medium text-slate-600">
                Sprache
              </span>
              <div className="flex gap-2">
                {(
                  [
                    ["de", "Deutsch"],
                    ["en", "Englisch"],
                    ["auto", "Automatisch"],
                  ] as const
                ).map(([val, label]) => (
                  <button
                    key={val}
                    onClick={() => setAnnLang(val)}
                    className={`rounded-lg border px-3 py-1.5 text-sm transition-colors ${
                      annLang === val
                        ? "border-slate-800 bg-slate-800 text-white"
                        : "border-slate-300 bg-white text-slate-600 hover:border-slate-400"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
              {annLang === "auto" && (
                <p className="text-xs text-slate-500">
                  Englisch, sobald mindestens die Hälfte der Spieler auf dem
                  Feld international ist – sonst Deutsch.
                </p>
              )}
            </div>

            {/* Stimmen */}
            <label className="block">
              <span className="mb-1 block text-sm font-medium text-slate-600">
                Deutsche Stimme
              </span>
              <select
                value={annVoiceDe}
                onChange={(e) => setAnnVoiceDe(e.currentTarget.value)}
                className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                           focus:border-slate-500 focus:outline-none"
              >
                <option value="">Standardstimme</option>
                {voicesForLang(voices, "de").map((v) => (
                  <option key={v.voiceURI} value={v.voiceURI}>
                    {v.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="block">
              <span className="mb-1 block text-sm font-medium text-slate-600">
                Englische Stimme
              </span>
              <select
                value={annVoiceEn}
                onChange={(e) => setAnnVoiceEn(e.currentTarget.value)}
                className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                           focus:border-slate-500 focus:outline-none"
              >
                <option value="">Standardstimme</option>
                {voicesForLang(voices, "en").map((v) => (
                  <option key={v.voiceURI} value={v.voiceURI}>
                    {v.name}
                  </option>
                ))}
              </select>
            </label>

            {/* Geschwindigkeit */}
            <label className="block">
              <span className="mb-1 block text-sm font-medium text-slate-600">
                Geschwindigkeit: {annRate.toFixed(1)}×
              </span>
              <input
                type="range"
                min={0.5}
                max={1.5}
                step={0.1}
                value={annRate}
                onChange={(e) => setAnnRate(Number(e.currentTarget.value))}
                className="w-full"
              />
            </label>

            {/* Gong */}
            <label className="flex items-center gap-2 text-sm text-slate-600">
              <input
                type="checkbox"
                checked={annGong}
                onChange={(e) => setAnnGong(e.currentTarget.checked)}
              />
              Gong vor der Ansage
            </label>

            {/* Test */}
            <div className="flex flex-col gap-1">
              <button
                onClick={() =>
                  void playTestAnnouncement(annLang === "en" ? "en" : "de", {
                    rate: annRate,
                    voiceURI:
                      (annLang === "en" ? annVoiceEn : annVoiceDe) || undefined,
                    gong: annGong,
                    nameOverrides: annNameOverrides,
                    nameOverridesEnabled: annOverridesEnabled,
                    // Azure nutzt den GESPEICHERTEN Key (Backend) — zum Testen
                    // einer neuen Stimme/Konfig vorher speichern.
                    azure: azureOption({
                      enabled: azEnabled,
                      region: azRegion,
                      key: azKey,
                      voice: azVoice,
                    }),
                  })
                }
                className="self-start rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium
                           text-slate-700 transition-colors hover:bg-slate-200"
              >
                Test-Ansage abspielen
              </button>
              <p className="text-xs text-slate-500">
                Vor dem Turnier einmal drücken – das schaltet die Tonausgabe
                am Rechner frei.
              </p>
            </div>

            {/* Aussprache-Korrekturen (Phonetik-Tabelle) */}
            <div className="flex flex-col gap-2 border-t border-slate-200 pt-4">
              <span className="text-sm font-medium text-slate-700">
                Aussprache-Korrekturen
              </span>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={annOverridesEnabled}
                  onChange={(e) => setAnnOverridesEnabled(e.currentTarget.checked)}
                />
                Aussprache-Korrekturen anwenden
              </label>
              <p className="text-xs text-slate-500">
                Ein mitgeliefertes <strong>Basis-Wörterbuch</strong> (
                {BASE_NAME_OVERRIDES.length} gängige internationale Nach- und
                Vornamen, u. a. vietnamesisch, chinesisch, indisch, französisch,
                spanisch, türkisch, polnisch) wird automatisch angewendet, damit
                die Stimme fremdsprachige Namen besser trifft. Eigene Korrekturen unten haben
                <strong> Vorrang</strong>: trage <strong>Name oder Namensteil</strong>
                und die gewünschte <strong>Aussprache</strong> (Lautschrift) ein –
                ein Nachname wie „Nguyen“ reicht einmal. Mit ▶ hörst du die
                Aussprache. (Korrekturen sind Näherungen, kein Ersatz für eine
                perfekte Lautschrift.)
              </p>

              {annNameOverrides.length > 0 && (
                <div className="flex flex-col gap-1.5">
                  {annNameOverrides.map((ov, idx) => (
                    <div key={idx} className="flex items-center gap-1.5">
                      <input
                        type="text"
                        value={ov.name}
                        placeholder="Name / Namensteil"
                        onChange={(e) => {
                          const v = e.currentTarget.value;
                          setAnnNameOverrides((prev) =>
                            prev.map((o, i) =>
                              i === idx ? { ...o, name: v } : o,
                            ),
                          );
                        }}
                        className="min-w-0 flex-1 rounded-md border border-slate-300 px-2 py-1 text-sm"
                      />
                      <span className="text-slate-400">→</span>
                      <input
                        type="text"
                        value={ov.say}
                        placeholder="Aussprache"
                        onChange={(e) => {
                          const v = e.currentTarget.value;
                          setAnnNameOverrides((prev) =>
                            prev.map((o, i) =>
                              i === idx ? { ...o, say: v } : o,
                            ),
                          );
                        }}
                        className="min-w-0 flex-1 rounded-md border border-slate-300 px-2 py-1 text-sm"
                      />
                      <button
                        type="button"
                        title="Aussprache testen"
                        onClick={() =>
                          void playNameTest(
                            ov.say || ov.name,
                            annLang === "en" ? "en" : "de",
                            {
                              rate: annRate,
                              voiceURI:
                                (annLang === "en" ? annVoiceEn : annVoiceDe) ||
                                undefined,
                            },
                          )
                        }
                        className="rounded-md bg-slate-100 px-2 py-1 text-sm text-slate-700 hover:bg-slate-200"
                      >
                        ▶
                      </button>
                      <button
                        type="button"
                        title="Zeile entfernen"
                        onClick={() =>
                          setAnnNameOverrides((prev) =>
                            prev.filter((_, i) => i !== idx),
                          )
                        }
                        className="rounded-md bg-slate-100 px-2 py-1 text-sm text-slate-500 hover:bg-rose-100 hover:text-rose-700"
                      >
                        ✕
                      </button>
                    </div>
                  ))}
                </div>
              )}

              <button
                type="button"
                onClick={() =>
                  setAnnNameOverrides((prev) => [...prev, { name: "", say: "" }])
                }
                className="self-start rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium
                           text-slate-700 transition-colors hover:bg-slate-200"
              >
                + Name hinzufügen
              </button>
            </div>

            {/* Azure Neural TTS (hochwertige Cloud-Stimme) */}
            <div className="flex flex-col gap-2 border-t border-slate-200 pt-4">
              <label className="flex items-center gap-2 text-sm font-medium text-slate-700">
                <input
                  type="checkbox"
                  checked={azEnabled}
                  onChange={(e) => setAzEnabled(e.currentTarget.checked)}
                />
                Hochwertige Stimme über Azure (Cloud)
              </label>
              <p className="text-xs text-slate-500">
                Spricht die ganze Ansage mit einer neuronalen Azure-Stimme und gibt
                asiatische/internationale Namen <strong>nativ</strong> wieder (per
                Sprach-Erkennung). Braucht Internet; bei Fehler/offline greift
                automatisch die lokale Stimme. Schlüssel + Region aus deiner
                Azure-Speech-Ressource. Wird nach dem Speichern aktiv.
              </p>
              {azEnabled && (
                <div className="flex flex-col gap-2">
                  <label className="text-sm text-slate-600">
                    Region
                    <input
                      type="text"
                      value={azRegion}
                      placeholder="westeurope"
                      onChange={(e) => setAzRegion(e.currentTarget.value)}
                      className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
                    />
                  </label>
                  <label className="text-sm text-slate-600">
                    Schlüssel (KEY 1)
                    <input
                      type="password"
                      value={azKey}
                      placeholder="Azure Speech Key"
                      onChange={(e) => setAzKey(e.currentTarget.value)}
                      className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
                    />
                  </label>
                  <label className="text-sm text-slate-600">
                    Stimme
                    <select
                      value={azVoice}
                      onChange={(e) => setAzVoice(e.currentTarget.value)}
                      className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
                    >
                      <option value="de-DE-SeraphinaMultilingualNeural">
                        Seraphina (weiblich, mehrsprachig)
                      </option>
                      <option value="de-DE-FlorianMultilingualNeural">
                        Florian (männlich, mehrsprachig)
                      </option>
                    </select>
                  </label>
                </div>
              )}
            </div>
          </div>
        )}
      </section>

      {/* Aufruf-Timer (1./2./3. Aufruf) */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={Timer}>Aufruf-Timer</SectionHeader>
        <p className="text-xs text-slate-500">
          Der Aufruf aufs Feld ist der <strong>1. Aufruf</strong>. bts-light
          zeigt dann je belegtem Feld eine hochzählende Uhr und meldet ab den
          eingestellten Minuten den <strong>2.</strong> und{" "}
          <strong>3./letzten</strong> Aufruf als fällig.
        </p>
        <label className="flex items-center gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={ctEnabled}
            onChange={(e) => setCtEnabled(e.currentTarget.checked)}
          />
          Aufruf-Timer aktivieren
        </label>

        {ctEnabled && (
          <div className="mt-1 flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4">
            <Field
              label="2. Aufruf nach (Minuten)"
              value={ctSecond}
              onChange={setCtSecond}
              type="number"
            />
            <Field
              label="3./letzter Aufruf nach (Minuten)"
              value={ctThird}
              onChange={setCtThird}
              type="number"
            />
            <p className="text-xs text-slate-500">
              Minuten ab dem 1. Aufruf. Beispiel: 2 und 4 → der 2. Aufruf wird
              nach 2 Minuten fällig, der letzte nach 4.
            </p>
            {Number(ctThird) <= Number(ctSecond) && (
              <p className="flex items-start gap-1.5 text-xs text-amber-700">
                <X size={14} className="mt-0.5 shrink-0" />
                Der 3. Aufruf sollte nach dem 2. liegen — beim Speichern wird er
                sonst automatisch angehoben.
              </p>
            )}
          </div>
        )}
      </section>

      {/* Automatische Feldvergabe */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={LayoutGrid}>Automatische Feldvergabe</SectionHeader>
        <p className="text-xs text-slate-500">
          Belegt freie Felder automatisch mit dem nächsten spielbereiten Spiel –
          in der Reihenfolge der BTP-Ansetzung (Zeitplan von oben nach unten),
          und überspringt Spiele, deren Spieler gerade spielen oder noch Pause
          haben. Schreibt die Zuweisung nach BTP (wie das Ziehen in der
          Spielübersicht). Gesperrte Felder bleiben frei.
        </p>
        <label className="flex items-center gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={aaEnabled}
            onChange={(e) => setAaEnabled(e.currentTarget.checked)}
          />
          Automatische Feldvergabe aktivieren
        </label>

        {aaEnabled && (
          <div className="mt-1 flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4">
            <Field
              label="Wartezeit, bis ein freies Feld belegt wird (Minuten)"
              value={aaWait}
              onChange={setAaWait}
              type="number"
            />
            <p className="text-xs text-slate-500">
              Verhindert, dass ein Feld sofort in der kurzen Lücke zwischen zwei
              Spielen belegt wird. 0 = sofort.{" "}
              {initialConfig.locked_courts.length > 0 && (
                <>Aktuell gesperrte Felder werden nicht automatisch belegt.</>
              )}
            </p>
            <Field
              label="Pause nach Spielende je Spieler (Minuten)"
              value={aaPause}
              onChange={setAaPause}
              type="number"
            />
            <p className="text-xs text-slate-500">
              Ein Spieler wird erst nach dieser Pause wieder automatisch
              aufgerufen. <strong>0 = Wert aus BTP übernehmen</strong> (BTP-
              Einstellung „Pause", Setting 1303). Unabhängig davon wird niemand
              aufgerufen, der gerade auf einem anderen Feld spielt.
            </p>
            <Field
              label="Aktive Halle (Tages-Halle, leer = alle)"
              value={aaActiveHall}
              onChange={setAaActiveHall}
              placeholder="z. B. Halle A"
            />
            <p className="text-xs text-slate-500">
              Nur für <strong>Mehr-Hallen-Turniere</strong>, bei denen an einem Tag
              nur in <strong>einer</strong> Halle gespielt wird (z. B. eine Datei für
              zwei Tage). Trägst du hier den Hallennamen ein (wie in BTP), verteilt
              die Auto-Vergabe nur auf diese Halle — <strong>ohne</strong> dass du
              Spiele erst „in Vorbereitung" rufen musst. Leer lassen bei
              Ein-Hallen-Turnieren.
            </p>
            <p className="flex items-start gap-1.5 text-xs text-amber-700">
              <Info size={14} className="mt-0.5 shrink-0" />
              Mehr-Hallen OHNE gesetzte aktive Halle: es werden nur Spiele verteilt,
              die du für die jeweilige Halle „in Vorbereitung" gerufen hast.
            </p>
          </div>
        )}
      </section>

      {/* Court-Monitor */}
      <section id="section-court-monitor" className="flex flex-col gap-2 scroll-mt-4">
        <SectionHeader icon={Monitor}>Court-Monitor</SectionHeader>
        <p className="text-xs text-slate-500">
          TV-Anzeige am Spielfeld (Raspberry Pi): Werbung im Leerlauf, die
          Match-Ansicht sobald ein Spiel aufs Feld kommt. Die Monitor-Adressen
          stehen auf der Tablet-Spielzettel-Seite.
        </p>
        <label className="flex items-center gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={cmEnabled}
            onChange={(e) => setCmEnabled(e.currentTarget.checked)}
          />
          Court-Monitor aktivieren
        </label>

        {cmEnabled && (
          <div className="mt-1 flex flex-col gap-4 rounded-xl border border-slate-200 bg-white p-4">
            {/* Werbebilder */}
            <div className="flex flex-col gap-2">
              <span className="text-sm font-medium text-slate-600">
                Werbebilder
              </span>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmAds}
                  onChange={(e) => setCmAds(e.currentTarget.checked)}
                />
                Werbung im Leerlauf anzeigen
              </label>
              <p className="text-xs text-slate-500">
                {cmAds
                  ? "Werden im Leerlauf nacheinander gezeigt – ein gemeinsamer Satz für alle Felder."
                  : "Aus: Ein freies Feld zeigt eine neutrale Leerlauf-Seite statt der Werbung."}
              </p>
              {ads.length > 0 && (
                <ul className="flex flex-col gap-1">
                  {ads.map((ad, i) => (
                    <li
                      key={ad.file}
                      className="flex items-center gap-2 rounded-lg border border-slate-200 px-2.5 py-1.5 text-sm"
                    >
                      <input
                        type="text"
                        value={ad.label}
                        placeholder={`Werbebild ${i + 1}`}
                        maxLength={80}
                        onChange={(e) => {
                          // Optimistisch lokal anwenden, damit der Operator
                          // beim Tippen direkt sieht; auf Blur persistieren.
                          const v = e.currentTarget.value;
                          setAds((prev) =>
                            prev.map((a) =>
                              a.file === ad.file ? { ...a, label: v } : a,
                            ),
                          );
                        }}
                        onBlur={(e) => {
                          void setCourtAdLabel(ad.file, e.currentTarget.value.trim());
                        }}
                        className="flex-1 min-w-0 rounded border border-transparent bg-transparent
                                   px-1.5 py-0.5 text-sm text-slate-700 placeholder:text-slate-400
                                   focus:border-slate-300 focus:bg-white focus:outline-none"
                      />
                      <button
                        onClick={() => void deleteAd(ad.file)}
                        title="Werbebild entfernen"
                        className="rounded p-1 text-slate-400 transition-colors
                                   hover:bg-rose-50 hover:text-rose-600"
                      >
                        <Trash2 size={15} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <button
                onClick={() => void pickAds()}
                className="self-start rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium
                           text-slate-700 transition-colors hover:bg-slate-200"
              >
                Werbebild hinzufügen …
              </button>
              {adError && <p className="text-xs text-rose-700">{adError}</p>}
            </div>

            {/* Wechsel-Intervall */}
            <label className="block">
              <span className="mb-1 block text-sm font-medium text-slate-600">
                Wechsel-Intervall: {cmInterval} s
              </span>
              <input
                type="range"
                min={3}
                max={30}
                step={1}
                value={cmInterval}
                onChange={(e) => setCmInterval(Number(e.currentTarget.value))}
                className="w-full"
              />
            </label>

            {/* Anzeige-Optionen */}
            <div className="flex flex-col gap-1.5">
              <span className="text-sm font-medium text-slate-600">Anzeige</span>
              <label className="block">
                <span className="mb-1 block text-xs text-slate-500">
                  Layout
                </span>
                <select
                  value={cmLayout}
                  onChange={(e) => setCmLayout(e.currentTarget.value)}
                  className="w-full rounded-lg border border-slate-300 bg-white
                             px-2.5 py-1.5 text-sm text-slate-700"
                >
                  <option value="split">A — Geteilt (oben/unten)</option>
                </select>
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmComboVertical}
                  onChange={(e) => setCmComboVertical(e.currentTarget.checked)}
                />
                Kombi-Anzeige: Felder <strong>nebeneinander</strong> (Hochformat
                je Feld – für einen TV zwischen zwei Feldern)
              </label>
              {/* Live-Vorschau: aktualisiert sich mit jeder Checkbox. */}
              <MonitorPreview
                showDiscipline={cmDiscipline}
                showRound={cmRound}
                showMatchNumber={cmMatchNumber}
                showTimer={cmTimer}
                showMatchClock={cmMatchClock}
              />
              <p className="mb-1 text-xs text-slate-500">
                Vorschau – ändert sich mit den Häkchen. Der Pausen-Countdown
                erscheint am Monitor nur während einer Spielpause.
              </p>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmDiscipline}
                  onChange={(e) => setCmDiscipline(e.currentTarget.checked)}
                />
                Disziplin in der Kopfzeile
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmRound}
                  onChange={(e) => setCmRound(e.currentTarget.checked)}
                />
                Runde in der Fußzeile
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmMatchNumber}
                  onChange={(e) => setCmMatchNumber(e.currentTarget.checked)}
                />
                Spielnummer in der Fußzeile
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmMatchClock}
                  onChange={(e) => setCmMatchClock(e.currentTarget.checked)}
                />
                Spieldauer in der Kopfzeile (Stoppuhr)
              </label>
              <label className="flex items-center gap-2 text-sm text-slate-600">
                <input
                  type="checkbox"
                  checked={cmTimer}
                  onChange={(e) => setCmTimer(e.currentTarget.checked)}
                />
                Pausen-Countdown (Retro-Klappanzeige)
              </label>
            </div>
          </div>
        )}
      </section>

      {/* Diagnose */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={Stethoscope}>Diagnose</SectionHeader>
        <label className="flex items-start gap-2 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={uploadLogs}
            onChange={(e) => setUploadLogs(e.currentTarget.checked)}
            className="mt-0.5"
          />
          <span>
            Diagnose-Logs automatisch an badhub senden – hilft, Fehler zu
            finden und zu beheben. Enthält nur technische Daten (keine
            Spielernamen).
          </span>
        </label>
      </section>

      {saveError && <p className="text-sm text-rose-700">{saveError}</p>}
      {saved && isSettings && (
        <p className="flex items-center gap-1.5 text-sm text-emerald-700">
          <Check size={16} /> Gespeichert – Liveticker neu gestartet.
        </p>
      )}

      <button
        onClick={saveAndStart}
        disabled={!canSave || saving}
        className="rounded-lg bg-slate-800 px-4 py-2.5 text-sm font-medium text-white
                   transition-colors hover:bg-slate-900 disabled:opacity-50"
      >
        {saving
          ? isSettings
            ? "Wird gespeichert …"
            : "Wird gestartet …"
          : isSettings
            ? "Speichern"
            : "Speichern & Liveticker starten"}
      </button>
    </main>
  );
}
