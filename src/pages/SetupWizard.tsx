import { type ReactNode, useState } from "react";
import {
  Check,
  Cloud,
  KeyRound,
  type LucideIcon,
  Server,
  Stethoscope,
  Target,
  Volume2,
  Wifi,
  X,
} from "lucide-react";
import { saveConfig, startSync, stopSync, testBtp } from "../api";
import { playTestAnnouncement } from "../io/announcer";
import { PRESETS, findPreset } from "../presets";
import { useAvailableVoices, voicesForLang } from "../state/useAvailableVoices";
import type { AnnounceLanguageMode, AppConfig, ConnectionMode } from "../types";

interface Props {
  initialConfig: AppConfig;
  onDone: (config: AppConfig) => void;
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
      {props.active && (
        <Check size={16} className="mt-1 shrink-0 text-slate-800" />
      )}
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

export function SetupWizard({ initialConfig, onDone }: Props) {
  const [presetId, setPresetId] = useState("bvbb");
  const [host, setHost] = useState(initialConfig.btp.host);
  const [port, setPort] = useState(String(initialConfig.btp.port));
  const [btpPassword, setBtpPassword] = useState(initialConfig.btp.password ?? "");
  const [badhubUrl, setBadhubUrl] = useState(initialConfig.badhub.url);
  const [badhubPassword, setBadhubPassword] = useState(initialConfig.badhub.password);
  const [badhubLiveUrl, setBadhubLiveUrl] = useState(initialConfig.badhub.live_url);
  const [uploadLogs, setUploadLogs] = useState(initialConfig.upload_logs);
  const [mode, setMode] = useState<ConnectionMode>(initialConfig.connection_mode);
  const [annEnabled, setAnnEnabled] = useState(initialConfig.announce.enabled);
  const [annLang, setAnnLang] = useState<AnnounceLanguageMode>(
    initialConfig.announce.language_mode,
  );
  const [annVoiceDe, setAnnVoiceDe] = useState(initialConfig.announce.voice_de);
  const [annVoiceEn, setAnnVoiceEn] = useState(initialConfig.announce.voice_en);
  const [annRate, setAnnRate] = useState(initialConfig.announce.rate);
  const [annGong, setAnnGong] = useState(initialConfig.announce.gong);
  const voices = useAvailableVoices();
  const [test, setTest] = useState<TestState>({ kind: "idle" });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  const isManual = presetId === MANUAL;

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
      connection_mode: mode,
      announce: {
        enabled: annEnabled,
        language_mode: annLang,
        voice_de: annVoiceDe,
        voice_en: annVoiceEn,
        rate: annRate,
        gong: annGong,
      },
    };
  }

  const canSave =
    host.trim() !== "" &&
    (!isManual || (badhubUrl.trim() !== "" && badhubPassword.trim() !== ""));

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
            BTS Light einrichten
          </h1>
          <p className="text-sm text-slate-500">
            Verbinde dein Turnier (BTP) mit dem Badhub-Liveticker.
          </p>
        </div>
      </header>

      {/* Schritt 1: Verband / Ziel */}
      <section className="flex flex-col gap-2">
        <SectionHeader icon={Target}>1 · Liveticker-Ziel</SectionHeader>
        {PRESETS.map((preset) => (
          <ChoiceCard
            key={preset.id}
            icon={Target}
            title={preset.label}
            description={preset.badhub.live_url}
            active={presetId === preset.id}
            onClick={() => setPresetId(preset.id)}
          />
        ))}
        <ChoiceCard
          icon={KeyRound}
          title="Anderes Turnier (manuell)"
          description="Badhub-URL und Passwort selbst eintragen"
          active={isManual}
          onClick={() => setPresetId(MANUAL)}
        />
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
          Wie erreichen die Schiedsrichter-Tablets diesen PC? Lässt sich
          später in den Einstellungen umstellen.
        </p>
        <ChoiceCard
          icon={Wifi}
          title="LAN – lokales Netz"
          description="Tablets verbinden sich direkt im Hallen-WLAN. Schnell und offline – braucht aber einen freigegebenen Port (Windows-Firewall)."
          active={mode === "lan"}
          onClick={() => setMode("lan")}
        />
        <ChoiceCard
          icon={Cloud}
          title="Über badhub.de – Cloud"
          description="Tablets und PC verbinden sich nur nach außen. Funktioniert auch hinter gesperrten Firmen-Firewalls – Internet vorausgesetzt."
          active={mode === "cloud"}
          onClick={() => setMode("cloud")}
        />
      </section>

      {/* Sprachansagen */}
      <section className="flex flex-col gap-2">
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

      <button
        onClick={saveAndStart}
        disabled={!canSave || saving}
        className="rounded-lg bg-slate-800 px-4 py-2.5 text-sm font-medium text-white
                   transition-colors hover:bg-slate-900 disabled:opacity-50"
      >
        {saving ? "Wird gestartet …" : "Speichern & Liveticker starten"}
      </button>
    </main>
  );
}
