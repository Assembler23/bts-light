// Ansage-Seiten im Browser (Spec `ansage-im-browser`, E1): Schalter
// „Audio für Ansage-Seiten erzeugen", Links und QR-Codes je Halle, wer gerade
// zuhört — und die Warnung, wenn in einer Halle doppelt gesprochen wird.
import { useEffect, useState } from "react";
import { Check, Copy, ExternalLink } from "lucide-react";
import { ansageHoerer, ansageLink, openExternal, saveConfig } from "../api";
import type { AnsageHoerer, AnsageLink, AppConfig } from "../types";

const TAKT_MS = 5000;

export function AnsageSeitenPanel({
  config,
  onSaved,
}: {
  config: AppConfig;
  onSaved: (config: AppConfig) => void;
}) {
  const an = config.announce.page_audio;
  const [links, setLinks] = useState<AnsageLink[]>([]);
  const [hoerer, setHoerer] = useState<AnsageHoerer>({ hallen: [], doppelbetrieb: [] });
  const [err, setErr] = useState("");
  const az = config.azure_tts;
  const azure = az.enabled && az.key.trim() !== "" && az.region.trim() !== "";

  useEffect(() => {
    if (!an) return;
    let active = true;
    const laden = () => {
      ansageLink().then((l) => active && setLinks(l)).catch(() => {});
      ansageHoerer().then((h) => active && setHoerer(h)).catch(() => {});
    };
    laden();
    const id = setInterval(laden, TAKT_MS);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [an]);

  async function umschalten(wert: boolean) {
    setErr("");
    const next: AppConfig = {
      ...config,
      announce: { ...config.announce, page_audio: wert },
    };
    try {
      await saveConfig(next);
      onSaved(next);
    } catch (e) {
      setErr(String(e));
    }
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4">
      <h2 className="text-sm font-semibold text-slate-700">Ansage-Seiten im Browser</h2>
      <p className="text-sm text-slate-500">
        Tablets, Handys oder ein Pi sprechen die Ansagen einer Halle — ohne
        eigenes Programm. Der Turnier-PC baut und vertont die Ansage, die Seite
        spielt sie nur ab. Das geht auch, wenn dieser PC selbst stumm bleibt.
      </p>
      <label className="flex items-center gap-2 text-sm text-slate-600">
        <input
          type="checkbox"
          checked={an}
          disabled={config.slave_mode}
          onChange={(e) => void umschalten(e.currentTarget.checked)}
        />
        Audio für Ansage-Seiten erzeugen
      </label>
      {config.slave_mode && (
        <p className="text-xs text-slate-500">
          Am Ansage-Slave nicht möglich — die Seiten hängen am Turnier-PC.
        </p>
      )}
      {err && <p className="text-xs text-rose-600">{err}</p>}

      {an && (
        <>
          {!azure && (
            <p className="rounded-lg border border-amber-200 bg-amber-50 p-2 text-xs text-amber-800">
              Ohne Azure liefert der PC kein Audio. Die Seiten sprechen dann mit
              der Stimme des Geräts — ein Gerät ohne Stimme gibt nur den Gong.
            </p>
          )}
          {hoerer.doppelbetrieb.length > 0 && (
            <p className="rounded-lg border border-rose-200 bg-rose-50 p-2 text-sm text-rose-800">
              Doppelt angesagt in{" "}
              <strong>{hoerer.doppelbetrieb.map((h) => h || "allen Hallen").join(", ")}</strong>
              : Dort spricht eine Ansage-Seite <em>und</em> ein PC (diese App mit
              „Ansagen aktivieren“ oder ein Ansage-Slave). Schalten Sie eines von
              beiden ab.
            </p>
          )}
          <p className="text-xs text-slate-500">
            Hört gerade zu:{" "}
            {hoerer.hallen.length === 0
              ? "niemand"
              : hoerer.hallen.map((h) => h || "alle Hallen").join(", ")}
          </p>
          {links.length === 0 ? (
            <p className="text-xs text-slate-500">
              Links gibt es nur im LAN-Betrieb, solange die Übertragung läuft.
            </p>
          ) : (
            <div className="flex flex-wrap gap-6">
              {links.map((l) => (
                <figure key={l.hall} className="flex flex-col items-center gap-2">
                  <figcaption className="text-sm font-medium text-slate-700">
                    {l.hall || "Ansage"}
                  </figcaption>
                  <div
                    className="w-40 bg-white p-2"
                    // Der QR-Code kommt als SVG aus dem Kern.
                    dangerouslySetInnerHTML={{ __html: l.qr_svg }}
                  />
                  <AdresseZeile url={l.url_tls || l.url} />
                </figure>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}

/** Adresse als Link (öffnet im Browser) samt Kopier-Knopf. */
function AdresseZeile({ url }: { url: string }) {
  const [fehler, setFehler] = useState("");
  const [kopiert, setKopiert] = useState(false);
  return (
    <span className="flex max-w-xs flex-col items-center gap-1">
      <span className="flex items-start gap-1">
        <button
          type="button"
          onClick={() => {
            setFehler("");
            openExternal(url).catch((err) => {
              setFehler(String(err));
              setTimeout(() => setFehler(""), 4000);
            });
          }}
          title="Im Browser öffnen"
          className="inline-flex items-start gap-1 break-all text-center font-mono text-[11px]
                     text-slate-600 underline decoration-slate-300 underline-offset-2
                     hover:text-sky-700 hover:decoration-sky-400"
        >
          {url}
          <ExternalLink size={12} className="mt-0.5 shrink-0" />
        </button>
        <button
          type="button"
          title="Adresse kopieren"
          onClick={() => {
            navigator.clipboard
              .writeText(url)
              .then(() => {
                setKopiert(true);
                setTimeout(() => setKopiert(false), 1500);
              })
              .catch(() => {});
          }}
          className="shrink-0 rounded-md p-1 text-slate-500 hover:bg-slate-100"
        >
          {kopiert ? <Check size={14} className="text-emerald-600" /> : <Copy size={14} />}
        </button>
      </span>
      {fehler && <span className="text-xs text-rose-600">{fehler}</span>}
    </span>
  );
}
