import { useState } from "react";
import { saveConfig, startSync, testBtp } from "../api";
import { PRESETS, findPreset } from "../presets";
import type { AppConfig, ConnectionMode } from "../types";

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
        className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm
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
      await startSync();
      onDone(config);
    } catch (e) {
      setSaveError(String(e));
      setSaving(false);
    }
  }

  return (
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-6 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold">BTS Light einrichten</h1>
        <p className="text-sm text-slate-500">
          Verbinde dein Turnier (BTP) mit dem Badhub-Liveticker.
        </p>
      </header>

      {/* Schritt 1: Verband / Ziel */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">1 · Liveticker-Ziel</h2>
        {PRESETS.map((preset) => (
          <button
            key={preset.id}
            onClick={() => setPresetId(preset.id)}
            className={`rounded-lg border px-4 py-3 text-left text-sm ${
              presetId === preset.id
                ? "border-slate-800 bg-slate-50"
                : "border-slate-300"
            }`}
          >
            <div className="font-medium">{preset.label}</div>
            <div className="text-xs text-slate-500">{preset.badhub.live_url}</div>
          </button>
        ))}
        <button
          onClick={() => setPresetId(MANUAL)}
          className={`rounded-lg border px-4 py-3 text-left text-sm ${
            isManual ? "border-slate-800 bg-slate-50" : "border-slate-300"
          }`}
        >
          <div className="font-medium">Anderes Turnier (manuell)</div>
          <div className="text-xs text-slate-500">
            Badhub-URL und Passwort selbst eintragen
          </div>
        </button>
      </section>

      {/* Schritt 2: BTP-Verbindung */}
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold text-slate-700">2 · BTP-Verbindung</h2>
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
          className="self-start rounded-lg bg-slate-200 px-3 py-1.5 text-sm
                     disabled:opacity-50"
        >
          {test.kind === "testing" ? "Teste …" : "Verbindung testen"}
        </button>
        {test.kind === "ok" && (
          <p className="text-sm text-green-700">
            ✓ BTP gefunden – Turnier „{test.tournament}"
          </p>
        )}
        {test.kind === "error" && (
          <p className="text-sm text-red-700">✗ {test.message}</p>
        )}
      </section>

      {/* Schritt 3: Badhub (nur manuell) */}
      {isManual && (
        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-slate-700">3 · Badhub-Zugang</h2>
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
        <h2 className="text-sm font-semibold text-slate-700">
          Tablet-Verbindung
        </h2>
        <p className="text-xs text-slate-500">
          Wie erreichen die Schiedsrichter-Tablets diesen PC? Lässt sich
          später in den Einstellungen umstellen.
        </p>
        <button
          onClick={() => setMode("lan")}
          className={`rounded-lg border px-4 py-3 text-left text-sm ${
            mode === "lan" ? "border-slate-800 bg-slate-50" : "border-slate-300"
          }`}
        >
          <div className="font-medium">LAN – lokales Netz</div>
          <div className="text-xs text-slate-500">
            Tablets verbinden sich direkt im Hallen-WLAN. Schnell und offline –
            braucht aber einen freigegebenen Port (Windows-Firewall).
          </div>
        </button>
        <button
          onClick={() => setMode("cloud")}
          className={`rounded-lg border px-4 py-3 text-left text-sm ${
            mode === "cloud"
              ? "border-slate-800 bg-slate-50"
              : "border-slate-300"
          }`}
        >
          <div className="font-medium">Über badhub.de – Cloud</div>
          <div className="text-xs text-slate-500">
            Tablets und PC verbinden sich nur nach außen. Funktioniert auch
            hinter gesperrten Firmen-Firewalls – Internet vorausgesetzt.
          </div>
        </button>
      </section>

      {/* Diagnose */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">Diagnose</h2>
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

      {saveError && <p className="text-sm text-red-700">{saveError}</p>}

      <button
        onClick={saveAndStart}
        disabled={!canSave || saving}
        className="rounded-lg bg-slate-800 px-4 py-2.5 text-sm font-medium text-white
                   disabled:opacity-50"
      >
        {saving ? "Wird gestartet …" : "Speichern & Liveticker starten"}
      </button>
    </main>
  );
}
