// Alle Ansage-Detail-Einstellungen (Sprache, Stimmen, Tempo, Gong, Aussprache-
// Korrekturen, Halle, Azure). Liegt auf der Seite „Ansagen" — in den
// Einstellungen wird das Ansage-Modul nur noch an-/ausgeschaltet. Speichert die
// geänderten Felder in die App-Konfiguration (announce + azure_tts).
import { useEffect, useState } from "react";
import { saveConfig, tabletOverview } from "../api";
import { playNameTest, playTestAnnouncement } from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
import { BASE_NAME_OVERRIDES } from "../io/nameOverrideBase";
import { useAvailableVoices, voicesForLang } from "../state/useAvailableVoices";
import type { AnnounceLanguageMode, AppConfig, NameOverride } from "../types";

export function AnnounceSettings({
  config,
  onSaved,
}: {
  config: AppConfig;
  onSaved: (config: AppConfig) => void;
}) {
  const voices = useAvailableVoices();
  const a = config.announce;
  const az = config.azure_tts;
  const [annLang, setAnnLang] = useState<AnnounceLanguageMode>(a.language_mode);
  const [annVoiceDe, setAnnVoiceDe] = useState(a.voice_de);
  const [annVoiceEn, setAnnVoiceEn] = useState(a.voice_en);
  const [annRate, setAnnRate] = useState(a.rate);
  const [annGong, setAnnGong] = useState(a.gong);
  const [annNameOverrides, setAnnNameOverrides] = useState<NameOverride[]>(
    a.name_overrides ?? [],
  );
  const [annOverridesEnabled, setAnnOverridesEnabled] = useState(
    a.name_overrides_enabled ?? true,
  );
  const [annHall, setAnnHall] = useState(a.announce_hall ?? "");
  const [azEnabled, setAzEnabled] = useState(az?.enabled ?? false);
  const [azRegion, setAzRegion] = useState(az?.region ?? "");
  const [azKey, setAzKey] = useState(az?.key ?? "");
  const [azVoice, setAzVoice] = useState(
    az?.voice ?? "de-DE-SeraphinaMultilingualNeural",
  );
  const [halls, setHalls] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [err, setErr] = useState("");

  // Hallen des Turniers (für die „Ansagen nur für Halle X"-Auswahl). Pollt,
  // damit die Auswahl auch nach der BTP-Verbindung erscheint.
  useEffect(() => {
    let active = true;
    const load = () => {
      tabletOverview()
        .then((info) => {
          if (!active) return;
          setHalls(
            [
              ...new Set(
                (info.courts ?? [])
                  .map((c) => c.location)
                  .filter((l) => l !== ""),
              ),
            ].sort((x, y) => x.localeCompare(y, "de")),
          );
        })
        .catch(() => {});
    };
    load();
    const id = setInterval(load, 15000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  async function save() {
    setSaving(true);
    setErr("");
    const next: AppConfig = {
      ...config,
      announce: {
        ...config.announce,
        language_mode: annLang,
        voice_de: annVoiceDe,
        voice_en: annVoiceEn,
        rate: annRate,
        gong: annGong,
        name_overrides: annNameOverrides
          .map((o) => ({ name: o.name.trim(), say: o.say.trim() }))
          .filter((o) => o.name && o.say),
        name_overrides_enabled: annOverridesEnabled,
        announce_hall: annHall.trim(),
        // Diese Form verwaltet die Blöcke nicht – aktuellen Stand bewahren
        // (sonst würde ein paralleler Block-Speichervorgang überschrieben).
        saved_announcements: config.announce.saved_announcements,
      },
      azure_tts: {
        enabled: azEnabled,
        region: azRegion.trim(),
        key: azKey.trim(),
        voice: azVoice,
      },
    };
    try {
      await saveConfig(next);
      onSaved(next);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 3000);
    } catch (e) {
      setErr(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Ansage-Einstellungen
        </h2>
        <div className="flex items-center gap-2">
          {saved && (
            <span className="text-xs font-medium text-emerald-600">
              Gespeichert ✓
            </span>
          )}
          <button
            onClick={() => void save()}
            disabled={saving}
            className="rounded-lg bg-slate-800 px-3.5 py-1.5 text-sm font-medium text-white
                       transition-colors hover:bg-slate-700 disabled:opacity-50"
          >
            Speichern
          </button>
        </div>
      </div>
      {err && (
        <div className="rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800">
          {err}
        </div>
      )}

      {/* Mehr-Hallen: nur eine Halle ansagen. Sichtbar bei ≥2 Hallen oder wenn
          schon eine gesetzt ist. */}
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
            Dieser PC sagt dann nur Spiele dieser Halle an. „Alle Hallen" = keine
            Einschränkung. So hört in einem 2-Hallen-Setup jede Halle nur ihre
            eigenen Ansagen.
          </p>
        </label>
      )}

      {/* Sprache */}
      <div className="flex flex-col gap-1.5">
        <span className="text-sm font-medium text-slate-600">Sprache</span>
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
            Englisch, sobald mindestens die Hälfte der Spieler auf dem Feld
            international ist – sonst Deutsch.
          </p>
        )}
      </div>

      {/* Stimmen — nur wählbar, solange die hochwertige Azure-Stimme AUS ist.
          Ist Azure an, spricht es die ganze Ansage und die Standard-Stimmen
          hätten keinen Effekt → ausblenden statt verwirren. */}
      {azEnabled ? (
        <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2 text-sm text-slate-600">
          Hochwertige <strong>Azure-Stimme</strong> ist aktiv – sie spricht alle
          Ansagen. Die Standard-Stimmenauswahl ist deshalb deaktiviert. (Bei
          Fehler/offline springt automatisch die lokale Stimme ein.)
        </div>
      ) : (
        <>
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
        </>
      )}

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
              voiceURI: (annLang === "en" ? annVoiceEn : annVoiceDe) || undefined,
              gong: annGong,
              nameOverrides: annNameOverrides,
              nameOverridesEnabled: annOverridesEnabled,
              // Azure nutzt den GESPEICHERTEN Key (Backend) — zum Testen einer
              // neuen Stimme/Konfig vorher speichern.
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
          Vor dem Turnier einmal drücken – das schaltet die Tonausgabe am Rechner
          frei. Für Azure die Einstellungen vorher speichern.
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
          {BASE_NAME_OVERRIDES.length} gängige internationale Nach- und Vornamen)
          wird automatisch angewendet, damit die Stimme fremdsprachige Namen
          besser trifft. Eigene Korrekturen unten haben <strong>Vorrang</strong>:
          trage <strong>Name oder Namensteil</strong> und die gewünschte
          <strong> Aussprache</strong> (Lautschrift) ein. Mit ▶ hörst du sie.
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
                      prev.map((o, i) => (i === idx ? { ...o, name: v } : o)),
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
                      prev.map((o, i) => (i === idx ? { ...o, say: v } : o)),
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
                    setAnnNameOverrides((prev) => prev.filter((_, i) => i !== idx))
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
          asiatische/internationale Namen <strong>nativ</strong> wieder. Braucht
          Internet; bei Fehler/offline greift automatisch die lokale Stimme.
          Schlüssel + Region aus deiner Azure-Speech-Ressource. Wird nach dem
          Speichern aktiv.
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
    </section>
  );
}
