// Alle Ansage-Detail-Einstellungen (Sprache, Stimmen, Tempo, Gong, Aussprache-
// Korrekturen, Halle, Azure). Liegt auf der Seite „Ansagen" — in den
// Einstellungen wird das Ansage-Modul nur noch an-/ausgeschaltet. Speichert die
// geänderten Felder in die App-Konfiguration (announce + azure_tts).
import { useEffect, useState } from "react";
import {
  saveConfig,
  sharePronunciations,
  tabletOverview,
  ttsDienstePruefen,
  ttsVerbrauch,
} from "../api";
import { playNameTest, playTestAnnouncement } from "../io/announcer";
import { azureOption, cloudStimmeVollstaendig } from "../io/azureAnnounce";
import { DIENSTE, dienstArt, dienstName } from "../io/sprachdienst.mjs";
import { useInheritedAzureVoice } from "../state/azureStatus";
import { BASE_NAME_OVERRIDES } from "../io/nameOverrideBase";
import {
  stimmenAuswahlWert,
  useStimmenOptionen,
} from "../state/useAvailableVoices";
import type {
  AnnounceLanguageMode,
  AppConfig,
  DienstPruefung,
  VerbrauchZeile,
  NameOverride,
} from "../types";

// Zugangsdaten der Dienste außer Azure (Spec `ansage-elevenlabs`). Die
// Dienstliste selbst steht in `io/sprachdienst.mjs` (Drift-Test gegen Rust).
type FremdArt = "elevenlabs" | "openai" | "gemini";

const ANBIETER: Record<
  FremdArt,
  {
    name: string;
    schluessel: string;
    platzhalter: string;
    land: string;
    stimmen: ReadonlyArray<readonly [string, string]>;
  }
> = {
  elevenlabs: {
    name: "ElevenLabs",
    schluessel: "ElevenLabs-Schlüssel",
    platzhalter: "sk_…",
    land: "ElevenLabs (USA)",
    // Mehrsprachige Standardstimmen; die erste ist die Voreinstellung (Rust
    // `ELEVENLABS_STANDARD_STIMME`).
    stimmen: [
      ["EXAVITQu4vr4xnSDxMaL", "Sarah (weiblich, ruhig)"],
      ["Xb7hH8MSUJpSbSDYk0k2", "Alice (weiblich, klar)"],
      ["XrExE9yKIg1WjnnlVkGX", "Matilda (weiblich, sachlich)"],
      ["pFZP5JQG7iQjIQuC4Bku", "Lily (weiblich, warm)"],
      ["JBFqnCBsd6RMkjVDRZzb", "George (männlich, warm)"],
      ["onwK4e9ZLuTAKqWW03F9", "Daniel (männlich, Sprecher)"],
      ["nPczCjzI2devNBz1zQrb", "Brian (männlich, tief)"],
      ["pqHfZKP75CvOlQylNhV4", "Bill (männlich, ruhig)"],
    ],
  },
  openai: {
    name: "OpenAI",
    schluessel: "OpenAI-API-Schlüssel",
    platzhalter: "sk-…",
    land: "OpenAI (USA)",
    stimmen: [
      ["coral", "Coral (weiblich, warm)"],
      ["nova", "Nova (weiblich, hell)"],
      ["shimmer", "Shimmer (weiblich, sanft)"],
      ["sage", "Sage (weiblich, ruhig)"],
      ["alloy", "Alloy (neutral)"],
      ["ash", "Ash (männlich, klar)"],
      ["onyx", "Onyx (männlich, tief)"],
      ["echo", "Echo (männlich, ruhig)"],
    ],
  },
  gemini: {
    name: "Gemini",
    schluessel: "Gemini-Schlüssel (Google AI Studio)",
    platzhalter: "AIza…",
    land: "Google (USA)",
    stimmen: [
      ["Kore", "Kore (weiblich, fest)"],
      ["Aoede", "Aoede (weiblich, luftig)"],
      ["Leda", "Leda (weiblich, jugendlich)"],
      ["Zephyr", "Zephyr (weiblich, hell)"],
      ["Charon", "Charon (männlich, sachlich)"],
      ["Orus", "Orus (männlich, fest)"],
      ["Puck", "Puck (männlich, munter)"],
      ["Fenrir", "Fenrir (männlich, lebhaft)"],
    ],
  },
};

export function AnnounceSettings({
  config,
  onSaved,
}: {
  config: AppConfig;
  onSaved: (config: AppConfig) => void;
}) {
  // Stimmen aus Windows (Kern), sonst aus dem Browser (ADR 0066).
  const stimmenDe = useStimmenOptionen("de");
  const stimmenEn = useStimmenOptionen("en");
  const inheritedVoice = useInheritedAzureVoice();
  const a = config.announce;
  const az = config.azure_tts;
  const [annLang, setAnnLang] = useState<AnnounceLanguageMode>(a.language_mode);
  const [annVoiceDe, setAnnVoiceDe] = useState(a.voice_de);
  const [annVoiceEn, setAnnVoiceEn] = useState(a.voice_en);
  const [annRate, setAnnRate] = useState(a.rate);
  const [annGong, setAnnGong] = useState(a.gong);
  const [annLegacy, setAnnLegacy] = useState(a.legacy_output === true);
  const [annNameOverrides, setAnnNameOverrides] = useState<NameOverride[]>(
    a.name_overrides ?? [],
  );
  const [annOverridesEnabled, setAnnOverridesEnabled] = useState(
    a.name_overrides_enabled ?? true,
  );
  const [annShare, setAnnShare] = useState(a.share_corrections ?? false);
  // Beide Schalter fehlen in Configs vor v0.9.246 — dann gilt „an", wie die
  // Rust-Seite es beim Laden ebenfalls einsetzt.
  const [annScorekeeper, setAnnScorekeeper] = useState(
    a.announce_scorekeeper !== false,
  );
  const [annUmpire, setAnnUmpire] = useState(a.announce_umpire !== false);
  const [annHall, setAnnHall] = useState(a.announce_hall ?? "");
  // Die Ansage-Halle ist das EINZIGE Feld dieser Form, das auch anderswo
  // gesetzt wird: vom Hallen-Wähler auf dem Dashboard und vom
  // Slave-Geräte-Panel (dort ist es zugleich „welche Halle bedient dieses
  // Gerät"). Ohne diesen Abgleich bliebe der lokale Stand auf dem Wert vom
  // Mounten stehen, und das nächste Speichern hier schriebe ihn zurück — die
  // eben woanders gewählte Halle wäre wieder abgewählt (Befund 23.08.2026).
  // Ein noch nicht gespeicherter Wert geht dabei verloren; das ist richtig so,
  // denn wirksam war er ohnehin nicht.
  useEffect(() => {
    setAnnHall(a.announce_hall ?? "");
  }, [a.announce_hall]);
  const [azEnabled, setAzEnabled] = useState(az?.enabled ?? false);
  const [azRegion, setAzRegion] = useState(az?.region ?? "");
  const [azKey, setAzKey] = useState(az?.key ?? "");
  const [azVoice, setAzVoice] = useState(
    az?.voice ?? "de-DE-SeraphinaMultilingualNeural",
  );
  // Optionale Stimme je Disziplin (Disziplin-Kürzel → Azure-Stimme). Leer =
  // Standard-Stimme. Wird nur bei aktivem Azure angeboten.
  const [azDiscVoices, setAzDiscVoices] = useState<Record<string, string>>(
    az?.discipline_voices ?? {},
  );
  // Sprachdienst + Zugänge der übrigen Dienste (Spec `ansage-elevenlabs`).
  const [azDienst, setAzDienst] = useState(az?.dienst || "azure");
  const [zugang, setZugang] = useState<Record<FremdArt, { key: string; voice: string }>>({
    elevenlabs: {
      key: az?.elevenlabs_key ?? "",
      voice: az?.elevenlabs_voice || ANBIETER.elevenlabs.stimmen[0][0],
    },
    openai: {
      key: az?.openai_key ?? "",
      voice: az?.openai_voice || ANBIETER.openai.stimmen[0][0],
    },
    gemini: {
      key: az?.gemini_key ?? "",
      voice: az?.gemini_voice || ANBIETER.gemini.stimmen[0][0],
    },
  });
  const art = dienstArt(azDienst);
  const fremd: FremdArt | null = art === "azure" ? null : art;
  const setzeZugang = (a: FremdArt, feld: "key" | "voice", wert: string) =>
    setZugang((z) => ({ ...z, [a]: { ...z[a], [feld]: wert } }));
  const [dienstMeldung, setDienstMeldung] = useState("");
  const [wechselLaeuft, setWechselLaeuft] = useState(false);
  // „Dienste prüfen": null = noch nicht geprüft.
  const [pruefung, setPruefung] = useState<DienstPruefung[] | null>(null);
  const [prueftGerade, setPrueftGerade] = useState(false);
  async function diensteProbieren() {
    setPrueftGerade(true);
    try {
      setPruefung(await ttsDienstePruefen());
    } catch (e) {
      setPruefung([
        { dienst: "", ok: false, eingerichtet: true, meldung: String(e), dauer_ms: 0 },
      ]);
    } finally {
      setPrueftGerade(false);
      void verbrauchLaden();
    }
  }
  // Zeichenzähler (E8): null = noch nicht geladen.
  const [verbrauch, setVerbrauch] = useState<VerbrauchZeile[] | null>(null);
  async function verbrauchLaden() {
    try {
      setVerbrauch(await ttsVerbrauch());
    } catch {
      setVerbrauch([]);
    }
  }
  useEffect(() => {
    void verbrauchLaden();
  }, []);
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

  // Schnellwechsel beim Turnier (E1): Die Auswahl gilt sofort, ohne den
  // Speichern-Knopf — aber NUR, wenn der Zieldienst im GESPEICHERTEN Stand
  // schon vollständig ist. Sonst würde ein Wechsel zum Schlüssel-Eintragen
  // sofort die Cloud-Stimme abschalten und allen fernen Hallen die geerbte
  // nehmen (Review 03.11.2026). Gespeichert wird nur der Dienst; halb
  // ausgefüllte andere Felder bleiben unberührt.
  async function dienstWechseln(dienst: string) {
    setAzDienst(dienst);
    setDienstMeldung("");
    const next: AppConfig = {
      ...config,
      azure_tts: { ...config.azure_tts, dienst },
    };
    if (!cloudStimmeVollstaendig(next.azure_tts)) {
      setDienstMeldung(
        config.azure_tts.enabled
          ? "Noch nicht aktiv: Zugang eintragen und speichern."
          : "Wird mit „Speichern“ aktiv.",
      );
      return;
    }
    setWechselLaeuft(true);
    try {
      await saveConfig(next);
      onSaved(next);
      setDienstMeldung("Gilt ab der nächsten Ansage.");
    } catch (e) {
      setDienstMeldung(`Nicht gespeichert: ${String(e)}`);
    } finally {
      setWechselLaeuft(false);
    }
  }

  async function save() {
    setSaving(true);
    setErr("");
    // Eine Korrektur ist gültig, wenn ein Name UND mindestens eine Korrektur
    // (Lautschrift ODER Sprach-Override) gesetzt ist.
    const cleanOverrides = annNameOverrides
      .map((o) => ({
        name: o.name.trim(),
        say: o.say.trim(),
        lang: (o.lang ?? "").trim(),
      }))
      .filter((o) => o.name && (o.say || o.lang));
    const next: AppConfig = {
      ...config,
      announce: {
        ...config.announce,
        language_mode: annLang,
        voice_de: annVoiceDe,
        voice_en: annVoiceEn,
        rate: annRate,
        gong: annGong,
        legacy_output: annLegacy,
        name_overrides: cleanOverrides,
        name_overrides_enabled: annOverridesEnabled,
        share_corrections: annShare,
        announce_scorekeeper: annScorekeeper,
        announce_umpire: annUmpire,
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
        // Nur nicht-leere Zuordnungen speichern (leer = Standard-Stimme).
        discipline_voices: Object.fromEntries(
          Object.entries(azDiscVoices).filter(([, v]) => v && v.trim() !== ""),
        ),
        dienst: azDienst,
        elevenlabs_key: zugang.elevenlabs.key.trim(),
        elevenlabs_voice: zugang.elevenlabs.voice.trim(),
        openai_key: zugang.openai.key.trim(),
        openai_voice: zugang.openai.voice.trim(),
        gemini_key: zugang.gemini.key.trim(),
        gemini_voice: zugang.gemini.voice.trim(),
      },
    };
    try {
      await saveConfig(next);
      onSaved(next);
      // Opt-in: eigene Korrekturen mit der Community teilen (fire-and-forget –
      // ein Fehler hier darf das Speichern nicht scheitern lassen).
      if (annShare && cleanOverrides.length > 0) {
        void sharePronunciations(cleanOverrides).catch(() => {});
      }
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
            disabled={saving || wechselLaeuft}
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
          Hochwertige <strong>Cloud-Stimme</strong> ({dienstName(azDienst)}) ist
          aktiv – sie spricht alle Ansagen. Die Standard-Stimmenauswahl ist deshalb deaktiviert. (Bei
          Fehler/offline springt automatisch die lokale Stimme ein.)
        </div>
      ) : (
        <>
          <label className="block">
            <span className="mb-1 block text-sm font-medium text-slate-600">
              Deutsche Stimme
            </span>
            <select
              value={stimmenAuswahlWert(annVoiceDe, stimmenDe)}
              onChange={(e) => setAnnVoiceDe(e.currentTarget.value)}
              className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                         focus:border-slate-500 focus:outline-none"
            >
              <option value="">Standardstimme</option>
              {stimmenDe.map((v) => (
                <option key={v.value} value={v.value}>
                  {v.label}
                </option>
              ))}
            </select>
          </label>
          <label className="block">
            <span className="mb-1 block text-sm font-medium text-slate-600">
              Englische Stimme
            </span>
            <select
              value={stimmenAuswahlWert(annVoiceEn, stimmenEn)}
              onChange={(e) => setAnnVoiceEn(e.currentTarget.value)}
              className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                         focus:border-slate-500 focus:outline-none"
            >
              <option value="">Standardstimme</option>
              {stimmenEn.map((v) => (
                <option key={v.value} value={v.value}>
                  {v.label}
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

      {/* Notfall-Schalter (Spec ansage-ausgabegeraet, D10, befristet): Falls
          die Ausgabe über Windows am Turniertag Probleme macht, ohne Update
          zum Weg bis v0.9.317 zurück. */}
      <label className="flex items-start gap-2 text-sm text-slate-600">
        <input
          type="checkbox"
          className="mt-0.5"
          checked={annLegacy}
          onChange={(e) => setAnnLegacy(e.currentTarget.checked)}
        />
        <span>
          Alter Abspielweg (über das Programmfenster)
          <span className="block text-xs text-slate-400">
            Nur im Notfall: Die Ansage folgt dann einem Wechsel des
            Ausgabegeräts (Klinke, USB-Box) nicht mehr von selbst.
          </span>
        </span>
      </label>

      {/* Wer außer den Spielern genannt wird. Beide Schalter wirken auf die
          Feldansage (automatisch wie manuell, inklusive Nachruf) — die
          eigenen Knöpfe „SR/AR ansagen" und „Bedienung nachrufen" auf der
          Spielübersicht bleiben unberührt. */}
      <label className="flex items-center gap-2 text-sm text-slate-600">
        <input
          type="checkbox"
          checked={annScorekeeper}
          onChange={(e) => setAnnScorekeeper(e.currentTarget.checked)}
        />
        Zähltafelbedienung mit ansagen
      </label>
      <label className="flex items-center gap-2 text-sm text-slate-600">
        <input
          type="checkbox"
          checked={annUmpire}
          onChange={(e) => setAnnUmpire(e.currentTarget.checked)}
        />
        Schiedsrichter mit ansagen
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
                discipline_voices: azDiscVoices,
                dienst: azDienst,
                elevenlabs_key: zugang.elevenlabs.key,
                openai_key: zugang.openai.key,
                gemini_key: zugang.gemini.key,
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
          frei. Für die Cloud-Stimme die Einstellungen vorher speichern.
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
          Die <strong>Sprache</strong> (Azure-Stimme) wird normalerweise automatisch
          erkannt; stell sie nur um, wenn ein Name falsch klingt — sie wirkt auf den
          <strong> vollständigen Namen</strong> dieser Zeile.
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
                <select
                  value={ov.lang ?? ""}
                  title="Sprache erzwingen (Azure-Stimme); „Auto“ = automatische Erkennung"
                  onChange={(e) => {
                    const v = e.currentTarget.value;
                    setAnnNameOverrides((prev) =>
                      prev.map((o, i) => (i === idx ? { ...o, lang: v } : o)),
                    );
                  }}
                  className="shrink-0 rounded-md border border-slate-300 px-1 py-1 text-sm text-slate-600"
                >
                  <option value="">Auto</option>
                  <option value="de">Deutsch</option>
                  <option value="cn">Chinesisch</option>
                  <option value="vn">Vietnamesisch</option>
                  <option value="es">Spanisch</option>
                  <option value="fr">Französisch</option>
                  <option value="pl">Polnisch</option>
                  <option value="tr">Türkisch</option>
                  <option value="ms">Malaiisch</option>
                  <option value="in">Indisch</option>
                </select>
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

        {/* Community-Wörterbuch: laden passiert immer automatisch; hier nur
            das opt-in Teilen der EIGENEN Korrekturen. */}
        <label className="mt-1 flex items-start gap-2 border-t border-slate-200 pt-3 text-sm text-slate-600">
          <input
            type="checkbox"
            checked={annShare}
            onChange={(e) => setAnnShare(e.currentTarget.checked)}
            className="mt-0.5"
          />
          <span>
            Meine Korrekturen mit der <strong>Community teilen</strong>
            <span className="block text-xs text-slate-500">
              Beim Speichern werden deine eigenen Aussprache-Einträge an die
              gemeinsame Badhub-Datenbank gesendet, damit andere Turniere sie
              automatisch nutzen. Das gemeinsame Wörterbuch wird ohnehin immer
              geladen – dieser Schalter steuert nur das Senden.
            </span>
          </span>
        </label>
      </div>

      {/* Azure Neural TTS (hochwertige Cloud-Stimme) */}
      <div className="flex flex-col gap-2 border-t border-slate-200 pt-4">
        <label className="flex items-center gap-2 text-sm font-medium text-slate-700">
          <input
            type="checkbox"
            checked={azEnabled}
            onChange={(e) => setAzEnabled(e.currentTarget.checked)}
          />
          Hochwertige Cloud-Stimme (Azure oder ElevenLabs)
        </label>
        <p className="text-xs text-slate-500">
          Spricht die ganze Ansage mit einer neuronalen Cloud-Stimme und gibt
          internationale Namen <strong>nativ</strong> wieder. Braucht Internet;
          bei Fehler/offline greift automatisch die lokale Stimme. Wird nach dem
          Speichern aktiv.
        </p>
        {azEnabled && (
          <label className="text-sm font-medium text-slate-700">
            Sprachdienst
            <select
              value={azDienst}
              disabled={wechselLaeuft || saving}
              onChange={(e) => void dienstWechseln(e.currentTarget.value)}
              className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
            >
              {DIENSTE.map((d) => (
                <option key={d.wert} value={d.wert}>
                  {d.name}
                </option>
              ))}
            </select>
            <span className="mt-1 block text-xs font-normal text-slate-500">
              Wechsel gilt sofort ab der nächsten Ansage – gedacht zum
              Vergleichen in der Halle.{" "}
              {dienstMeldung && <strong>{dienstMeldung}</strong>}
            </span>
          </label>
        )}
        {azEnabled && (
          <div className="flex flex-col gap-1.5 rounded-lg border border-slate-200 bg-slate-50 p-2.5">
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => void diensteProbieren()}
                disabled={prueftGerade}
                className="rounded-lg bg-slate-100 px-3 py-1 text-sm font-medium text-slate-700
                           ring-1 ring-slate-200 transition-colors hover:bg-slate-200 disabled:opacity-50"
              >
                {prueftGerade ? "Prüfe …" : "Dienste prüfen"}
              </button>
              <span className="text-xs text-slate-500">
                Kurze Probe an jeden Dienst mit gespeichertem Schlüssel – zeigt,
                ob Schlüssel und Kontingent noch reichen.
              </span>
            </div>
            {pruefung && (
              <ul className="flex flex-col gap-0.5 text-xs">
                {pruefung
                  .filter((p) => p.eingerichtet)
                  .map((p) => (
                    <li
                      key={p.dienst || "fehler"}
                      className={p.ok ? "text-emerald-700" : "text-rose-700"}
                    >
                      {p.ok ? "✓" : "✗"} <strong>{p.dienst ? dienstName(p.dienst) : "Prüfung"}</strong>
                      {": "}
                      {p.meldung}
                      {p.ok && ` (${(p.dauer_ms / 1000).toFixed(1)} s)`}
                    </li>
                  ))}
                {pruefung.every((p) => !p.eingerichtet) && (
                  <li className="text-slate-500">
                    Noch kein Dienst eingerichtet – Schlüssel eintragen und speichern.
                  </li>
                )}
              </ul>
            )}
            <div className="mt-1 flex items-center gap-2">
              <span className="text-xs font-medium text-slate-700">Verbrauch der Cloud-Stimme</span>
              <button
                type="button"
                onClick={() => void verbrauchLaden()}
                className="rounded px-1.5 text-xs text-slate-500 ring-1 ring-slate-200 hover:bg-slate-100"
              >
                Aktualisieren
              </button>
            </div>
            {verbrauch && verbrauch.length === 0 && (
              <p className="text-xs text-slate-500">Noch nichts gezählt.</p>
            )}
            {verbrauch && verbrauch.length > 0 && (
              <table className="text-xs text-slate-700">
                <thead>
                  <tr className="text-left text-slate-500">
                    <th className="pr-3 font-normal">Tag</th>
                    <th className="pr-3 font-normal">Dienst</th>
                    <th className="pr-3 text-right font-normal">Zeichen</th>
                    <th className="pr-3 text-right font-normal">neu vertont</th>
                    <th className="text-right font-normal">aus Cache</th>
                  </tr>
                </thead>
                <tbody>
                  {verbrauch.map((z) => (
                    <tr key={`${z.tag}|${z.dienst}`}>
                      <td className="pr-3">{z.tag.split("-").reverse().join(".")}</td>
                      <td className="pr-3">{dienstName(z.dienst)}</td>
                      <td className="pr-3 text-right tabular-nums">
                        {z.zeichen.toLocaleString("de-DE")}
                      </td>
                      <td className="pr-3 text-right tabular-nums">{z.neu}</td>
                      <td className="text-right tabular-nums">{z.aus_cache}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <span className="text-xs text-slate-500">
              Nur dieser PC, letzte 14 Tage. Gezählt wird der gesprochene Text;
              ElevenLabs rechnet je Zeichen ab (Flash zur Hälfte), OpenAI und
              Gemini nach Token bzw. Audiolänge.
            </span>
          </div>
        )}
        {/* Cloud-Slave: Azure-Zugang kommt vom Master (ADR 0003) — dann sind
            lokale Eingaben optional und dienen nur als Übersteuerung. */}
        {inheritedVoice && (
          <p className="rounded-lg bg-emerald-50 px-3 py-2 text-xs text-emerald-800">
            <strong>Vom Master geerbt ✓</strong> — dieser PC nutzt die
            Cloud-Stimme des Masters ({inheritedVoice}). Es sind
            keine Eingaben nötig; lokale, vollständige Eingaben hätten Vorrang.
          </p>
        )}
        {/* Genau der stumme Turnier-Bug: Schalter an, aber Key/Region fehlen →
            ohne Vererbung bliebe nur die Standardstimme. Sichtbar warnen. */}
        {azEnabled && fremd && !inheritedVoice && !zugang[fremd].key.trim() && (
          <p className="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">
            <strong>Unvollständig:</strong> Ohne {ANBIETER[fremd].schluessel} bleibt
            die Cloud-Stimme aus — es spricht die Standardstimme.
          </p>
        )}
        {azEnabled && fremd && (
          <div className="flex flex-col gap-2">
            <label className="text-sm text-slate-600">
              {ANBIETER[fremd].schluessel}
              <input
                type="password"
                value={zugang[fremd].key}
                placeholder={ANBIETER[fremd].platzhalter}
                onChange={(e) => setzeZugang(fremd, "key", e.currentTarget.value)}
                className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
              />
            </label>
            <label className="text-sm text-slate-600">
              Stimme
              <select
                value={
                  ANBIETER[fremd].stimmen.some(([id]) => id === zugang[fremd].voice)
                    ? zugang[fremd].voice
                    : "eigene"
                }
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  setzeZugang(fremd, "voice", v === "eigene" ? "" : v);
                }}
                className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
              >
                {ANBIETER[fremd].stimmen.map(([id, name]) => (
                  <option key={id} value={id}>
                    {name}
                  </option>
                ))}
                <option value="eigene">Andere Stimme …</option>
              </select>
            </label>
            {!ANBIETER[fremd].stimmen.some(([id]) => id === zugang[fremd].voice) && (
              <label className="text-sm text-slate-600">
                Stimmen-Name oder -ID
                <input
                  type="text"
                  value={zugang[fremd].voice}
                  placeholder={ANBIETER[fremd].stimmen[0][0]}
                  onChange={(e) => setzeZugang(fremd, "voice", e.currentTarget.value)}
                  className="mt-1 w-full rounded-md border border-slate-300 px-2 py-1 text-sm"
                />
              </label>
            )}
            <p className="text-xs text-slate-500">
              {ANBIETER[fremd].name} bekommt die Namen so, wie sie in BTP stehen;
              es gelten nur deine eigenen Aussprache-Korrekturen. Die
              Spielernamen gehen dabei an {ANBIETER[fremd].land}. Jede neue
              Ansage kostet beim Anbieter; Wiederholungen kommen aus dem Cache.
              Nur einen eingeschränkten Schlüssel mit Ausgabenlimit eintragen –
              ferne Hallen bekommen ihn über die Cloud mit.
              {fremd === "gemini" &&
                " Mit einem kostenlosen AI-Studio-Schlüssel darf Google die Namen zur Produktverbesserung nutzen, und es gibt nur wenige Anfragen pro Minute – für das Turnier die Abrechnung aktivieren."}
            </p>
          </div>
        )}
        {azEnabled && !fremd && !inheritedVoice && (!azKey.trim() || !azRegion.trim()) && (
          <p className="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">
            <strong>Unvollständig:</strong> Ohne{" "}
            {!azKey.trim() && !azRegion.trim()
              ? "Schlüssel und Region"
              : !azKey.trim()
                ? "Schlüssel"
                : "Region"}{" "}
            bleibt die Azure-Stimme aus — es spricht die Standardstimme.
          </p>
        )}
        {azEnabled && !fremd && (
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

            {/* Optional: Stimme je Disziplin. Leer = Standard-Stimme oben. */}
            <div className="flex flex-col gap-1.5 rounded-lg border border-slate-200 bg-slate-50 p-2.5">
              <span className="text-sm font-medium text-slate-700">
                Stimme je Disziplin (optional)
              </span>
              <p className="text-xs text-slate-500">
                Leer = Standard-Stimme oben. So kann z. B. Herreneinzel/-doppel
                von der männlichen und die Damen-Disziplinen von der weiblichen
                Stimme angesagt werden — frei pro Disziplin.
              </p>
              {(
                [
                  ["mens_singles", "Herreneinzel"],
                  ["mens_doubles", "Herrendoppel"],
                  ["womens_singles", "Dameneinzel"],
                  ["womens_doubles", "Damendoppel"],
                  ["mixed", "Mixed"],
                ] as const
              ).map(([key, label]) => (
                <label
                  key={key}
                  className="flex items-center justify-between gap-2 text-sm text-slate-600"
                >
                  {label}
                  <select
                    value={azDiscVoices[key] ?? ""}
                    onChange={(e) => {
                      const v = e.currentTarget.value;
                      setAzDiscVoices((prev) => {
                        const next = { ...prev };
                        if (v) next[key] = v;
                        else delete next[key];
                        return next;
                      });
                    }}
                    className="rounded-md border border-slate-300 px-2 py-1 text-sm"
                  >
                    <option value="">Standard</option>
                    <option value="de-DE-SeraphinaMultilingualNeural">
                      Seraphina (weiblich)
                    </option>
                    <option value="de-DE-FlorianMultilingualNeural">
                      Florian (männlich)
                    </option>
                  </select>
                </label>
              ))}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
