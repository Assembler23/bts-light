import { useCallback, useEffect, useState } from "react";
import { Check, ClipboardList, Copy, Plus, Trash2, X } from "lucide-react";
import {
  getStatus,
  tlDeviceAdd,
  tlDeviceRemove,
  tlWebInfo,
  tlWebSetEnabled,
} from "../api";
import type { AppConfig, TlPairing, TlWebInfo } from "../types";

interface Props {
  /** Meldet die neue Konfiguration nach oben. **Nicht optional:** Ohne das
   *  bliebe die Kopie der App veraltet, und der nächste Speichervorgang aus
   *  den Einstellungen schriebe den alten `tl_web`-Stand zurück — alle
   *  Kopplungen wären weg, ihre Zugänge unwiederbringlich. */
  onConfigSaved: (config: AppConfig) => void;
}

/**
 * Geräteverwaltung der Turnierleitungs-Oberfläche.
 *
 * Hier entstehen die Zugänge und hier werden sie entzogen — mehr braucht der
 * Widerruf nicht: Was der Turnier-PC nicht mehr nennt, gilt nicht mehr, im
 * Hallennetz wie über den Relay.
 *
 * Der Zugang eines Geräts ist **genau einmal** sichtbar: beim Koppeln.
 * Danach gibt es keinen Weg mehr, ihn anzuzeigen, auch nicht für die
 * Turnierleitung. Wer sein Gerät verliert, koppelt neu — das ist der kürzere
 * Weg als ein Zugang, der dauerhaft in einer Liste steht.
 */
export function TlWebPanel({ onConfigSaved }: Props) {
  const [info, setInfo] = useState<TlWebInfo | null>(null);
  const [fehler, setFehler] = useState("");
  const [name, setName] = useState("");
  const [neu, setNeu] = useState<TlPairing | null>(null);
  const [busy, setBusy] = useState(false);

  // Läuft die Übertragung? Ohne sie horcht weder der Server im Hallennetz
  // noch besteht die Verbindung zum Relay — die Adresse im QR ginge ins
  // Leere, und der Zugang ist nur dieses eine Mal zu sehen.
  const [laeuft, setLaeuft] = useState(true);

  const laden = useCallback(() => {
    tlWebInfo()
      .then(setInfo)
      .catch((e) => setFehler(String(e)));
    getStatus()
      .then((s) => setLaeuft(s.running))
      .catch(() => setLaeuft(true));
  }, []);

  useEffect(laden, [laden]);

  const koppeln = async () => {
    const bezeichnung = name.trim();
    if (!bezeichnung) {
      setFehler("Bitte einen Namen vergeben — er steht später im Protokoll.");
      return;
    }
    setBusy(true);
    setFehler("");
    // Den alten QR sofort wegnehmen: Bliebe er bei einem Fehlschlag stehen,
    // scannte jemand ihn für das neue Gerät — und zwei Tablets teilten sich
    // einen Zugang, von dem der Widerruf nur einen trifft.
    setNeu(null);
    try {
      const [pairing, config] = await tlDeviceAdd(bezeichnung, "");
      setNeu(pairing);
      onConfigSaved(config);
      setName("");
      laden();
    } catch (e) {
      setFehler(String(e));
    } finally {
      setBusy(false);
    }
  };

  const entziehen = async (id: string) => {
    setFehler("");
    try {
      onConfigSaved(await tlDeviceRemove(id));
      laden();
    } catch (e) {
      setFehler(String(e));
    }
  };

  const umschalten = async (an: boolean) => {
    setFehler("");
    try {
      onConfigSaved(await tlWebSetEnabled(an));
      laden();
    } catch (e) {
      setFehler(String(e));
    }
  };

  const voll = !!info && info.devices.length >= info.max_devices;

  return (
    <main className="mx-auto flex min-h-full max-w-4xl flex-col gap-5 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <ClipboardList className="h-7 w-7 text-slate-400" />
        <div className="flex-1">
          <h1 className="text-2xl font-semibold leading-tight">Turnierleitung</h1>
          <p className="text-sm text-slate-500">
            Die Weboberfläche zum Vergeben der Felder — auf Tablet, Telefon oder
            einem zweiten Rechner. Jedes Gerät bekommt einen eigenen Zugang, den
            Sie jederzeit entziehen können.
          </p>
        </div>
      </header>

      {fehler && (
        <p className="rounded-xl border border-rose-200 bg-rose-50 p-4 text-sm text-rose-800">
          {fehler}
        </p>
      )}

      {/* Schalter. Abschalten behält die Geräte — ein versehentlicher Klick
          soll nicht bedeuten, dass alle Tablets neu gescannt werden müssen. */}
      <section className="flex items-center gap-4 rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
        <div className="flex-1">
          <h2 className="font-medium">
            {info?.enabled ? "Oberfläche ist freigeschaltet" : "Oberfläche ist aus"}
          </h2>
          <p className="text-sm text-slate-500">
            {info?.enabled
              ? "Gekoppelte Geräte können Felder vergeben, Spiele aufrufen und Ergebnisse eintragen."
              : "Solange sie aus ist, wird jede Anfrage abgewiesen — auch von gekoppelten Geräten. Die Kopplungen bleiben erhalten."}
          </p>
        </div>
        <button
          type="button"
          onClick={() => umschalten(!info?.enabled)}
          className={`rounded-lg px-4 py-2 text-sm font-medium ${
            info?.enabled
              ? "border border-slate-300 bg-white text-slate-700 hover:bg-slate-50"
              : "bg-slate-900 text-white hover:bg-slate-700"
          }`}
        >
          {info?.enabled ? "Abschalten" : "Freischalten"}
        </button>
      </section>

      {/* Koppeln. */}
      <section className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
        <h2 className="font-medium">Gerät koppeln</h2>
        <p className="text-sm text-slate-500">
          Namen eintragen, dann den QR-Code auf dem Gerät scannen. Die Seite
          öffnet sich angemeldet; einen Code muss niemand abtippen.
        </p>
        {!laeuft && (
          <p className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
            Die Übertragung läuft gerade nicht — die Adresse im QR-Code ist
            erst nach dem Start erreichbar. Am besten zuerst starten: Der
            Zugang ist nur einmal zu sehen, und ein Scan ins Leere verbrennt
            ihn.
          </p>
        )}
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-slate-600">Name des Geräts</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void koppeln()}
              placeholder="Tablet Meeting Point"
              className="w-72 rounded-lg border border-slate-300 px-3 py-2"
            />
          </label>
          <button
            type="button"
            onClick={koppeln}
            disabled={busy || voll}
            className="inline-flex items-center gap-1.5 rounded-lg bg-slate-900 px-4 py-2 text-sm font-medium text-white hover:bg-slate-700 disabled:opacity-50"
          >
            <Plus className="h-4 w-4" />
            {busy ? "Wird gekoppelt …" : "Koppeln"}
          </button>
        </div>
        {voll && (
          <p className="text-sm text-amber-700">
            Die Liste fasst {info?.max_devices} Kopplungen — bitte zuerst eine
            alte entfernen.
          </p>
        )}
      </section>

      {/* Die Zugänge eines frisch gekoppelten Geräts. */}
      {neu && (
        <section className="flex flex-col gap-3 rounded-xl border border-emerald-200 bg-emerald-50 p-5">
          <div className="flex items-start gap-3">
            <div className="flex-1">
              <h2 className="font-medium text-emerald-900">
                Jetzt scannen — dieser Zugang ist nur jetzt zu sehen
              </h2>
              <p className="text-sm text-emerald-800">
                Er lässt sich später nicht erneut anzeigen. Klappt das Scannen
                nicht, können Sie die Adresse darunter auch abtippen. Geht das
                Gerät verloren, entziehen Sie ihm den Zugang und koppeln neu.
              </p>
            </div>
            <button
              type="button"
              onClick={() => setNeu(null)}
              aria-label="Schließen"
              className="rounded-lg p-1.5 text-emerald-700 hover:bg-emerald-100"
            >
              <X className="h-5 w-5" />
            </button>
          </div>
          <div className="flex flex-wrap justify-center gap-6">
            {neu.entrances.map((e) => (
              <figure key={e.label} className="flex flex-col items-center gap-2">
                <figcaption className="text-sm font-medium text-emerald-900">
                  {e.label}
                </figcaption>
                <div
                  className="bg-white p-3"
                  // Der QR-Code kommt als SVG aus dem Kern — erzeugt auf
                  // diesem Rechner, damit der Zugang ihn nicht verlässt.
                  dangerouslySetInnerHTML={{ __html: e.qr_svg }}
                />
                {/* Die Adresse zusätzlich als Text und zum Kopieren: Ohne sie
                    stünde man bei einer nicht scannbaren Kamera vor einem
                    verbrannten Zugang — und wer am selben Rechner testet,
                    scannt gar nicht, sondern kopiert. */}
                <div className="flex max-w-xs items-start gap-1">
                  <code className="break-all text-center text-[11px] text-emerald-800">
                    {e.url}
                  </code>
                  <CopyUrlButton url={e.url} />
                </div>
              </figure>
            ))}
          </div>
        </section>
      )}

      {/* Die gekoppelten Geräte. */}
      <section className="flex flex-col gap-2">
        <h2 className="font-medium">
          Gekoppelte Geräte{" "}
          <span className="text-sm font-normal text-slate-500">
            ({info?.devices.length ?? 0})
          </span>
        </h2>
        <p className="text-xs text-slate-500">
          Gleichzeitig bedienen können {info?.max_online ?? 8} Geräte die
          Oberfläche. Weitere werden abgewiesen, bis ein Platz frei wird — ein
          geschlossener Tab gibt seinen nach einer Minute selbst frei.
        </p>
        {info && info.devices.length === 0 && (
          <p className="rounded-xl border border-slate-200 bg-white p-5 text-sm text-slate-500 shadow-sm">
            Noch kein Gerät gekoppelt.
          </p>
        )}
        {info?.devices.map((d) => (
          <DeviceRow key={d.id} label={d.label} createdAtMs={d.created_at_ms}
            onRevoke={() => entziehen(d.id)} />
        ))}
      </section>
    </main>
  );
}

/**
 * Eine Zeile der Geräteliste — mit Rückfrage **in der Seite**.
 *
 * Kein `window.confirm`: Ein natives Fenster blockiert die ganze App, sieht
 * auf jedem Betriebssystem anders aus und passt zu keiner anderen
 * Bestätigung in bts-light.
 */
function DeviceRow({
  label,
  createdAtMs,
  onRevoke,
}: {
  label: string;
  createdAtMs: number;
  onRevoke: () => void;
}) {
  const [fragt, setFragt] = useState(false);
  return (
    <div className="flex items-center gap-3 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
      <div className="flex-1">
        <div className="font-medium">{label}</div>
        <div className="text-xs text-slate-500">
          gekoppelt am{" "}
          {new Date(createdAtMs).toLocaleDateString("de-DE", {
            day: "2-digit",
            month: "2-digit",
            year: "numeric",
          })}
        </div>
      </div>
      {fragt ? (
        <div className="flex items-center gap-2">
          <span className="text-sm text-slate-600">Zugang entziehen?</span>
          <button
            type="button"
            onClick={() => {
              setFragt(false);
              onRevoke();
            }}
            className="rounded-lg bg-rose-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-rose-700"
          >
            Ja, sperren
          </button>
          <button
            type="button"
            onClick={() => setFragt(false)}
            className="rounded-lg border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50"
          >
            Abbrechen
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setFragt(true)}
          className="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:border-rose-300 hover:bg-rose-50 hover:text-rose-700"
        >
          <Trash2 className="h-4 w-4" />
          Zugang entziehen
        </button>
      )}
    </div>
  );
}

/** Kleiner Button, der die Zugangs-Adresse in die Zwischenablage kopiert —
 *  für den Test am selben Rechner, wo niemand einen QR-Code scannt.
 *  Gleiche Mechanik wie in TabletPanel/CourtMonitorPanel. */
function CopyUrlButton({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* Zwischenablage nicht verfügbar – ignorieren */
    }
  }
  return (
    <button
      onClick={copy}
      title="Adresse kopieren"
      className="shrink-0 rounded-md p-1 text-emerald-700 transition-colors hover:bg-emerald-100"
    >
      {copied ? <Check size={14} className="text-emerald-600" /> : <Copy size={14} />}
    </button>
  );
}
