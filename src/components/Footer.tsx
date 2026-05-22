import { useEffect, useState } from "react";
import { Heart, X } from "lucide-react";
import { appVersion, openExternal } from "../api";

/**
 * Mitwirkende der BTS-Linie, auf der bts-light aufbaut. Die Reihenfolge
 * folgt der Projektgeschichte: Idee zuerst.
 */
const CREDITS = [
  {
    name: "Philipp Hagemeister",
    url: "https://github.com/phihag",
    role: "Visionär einer digitalen Turnierausrichtung.",
  },
  {
    name: "Tim Lehr",
    url: "https://github.com/tlehr",
    role: "Pflege und Weiterentwicklung von BTS.",
  },
  {
    name: "letilo",
    url: "https://github.com/letilo",
    role: "Liveticker-Anbindung an badhub.de, auf der bts-light aufbaut.",
  },
];

/**
 * Schmale Fußzeile mit der installierten Version und einem Über-Dialog,
 * der die Mitwirkenden der BTS-Community würdigt.
 */
export function Footer() {
  const [version, setVersion] = useState("");
  const [open, setOpen] = useState(false);

  useEffect(() => {
    appVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);

  return (
    <>
      <footer className="flex shrink-0 items-center justify-between border-t border-slate-200 bg-white px-4 py-1.5 text-xs text-slate-400">
        <span>BTS Light{version && ` v${version}`}</span>
        <button
          onClick={() => setOpen(true)}
          className="transition-colors hover:text-slate-600"
        >
          Über &amp; Mitwirkende
        </button>
      </footer>

      {open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4">
          <div className="w-full max-w-md overflow-hidden rounded-xl bg-white shadow-xl">
            <div className="flex items-center gap-2 border-b border-slate-200 px-5 py-3">
              <h2 className="flex-1 font-semibold text-slate-800">
                Über BTS Light
              </h2>
              <button
                onClick={() => setOpen(false)}
                className="text-slate-400 transition-colors hover:text-slate-600"
                title="Schließen"
              >
                <X size={18} />
              </button>
            </div>

            <div className="px-5 py-4 text-sm text-slate-700">
              <p className="text-slate-500">
                Version {version || "–"} · Plug-and-play-Brücke zwischen BTP
                und dem badhub.de-Liveticker.
              </p>

              <div className="mt-4 flex items-center gap-1.5 font-semibold text-slate-800">
                <Heart size={15} className="text-rose-500" />
                Mitwirkende &amp; Dank
              </div>
              <p className="mt-1 text-slate-600">
                bts-light steht auf den Schultern der BTS-Community. Großer
                Dank an die, die das möglich gemacht haben:
              </p>
              <ul className="mt-3 flex flex-col gap-2.5">
                {CREDITS.map((c) => (
                  <li
                    key={c.url}
                    className="rounded-lg border border-slate-200 px-3 py-2"
                  >
                    <button
                      onClick={() => void openExternal(c.url)}
                      className="font-medium text-emerald-700 hover:underline"
                    >
                      {c.name}
                    </button>
                    <p className="mt-0.5 text-slate-600">{c.role}</p>
                  </li>
                ))}
              </ul>
            </div>

            <div className="flex justify-end border-t border-slate-200 bg-slate-50 px-5 py-3">
              <button
                onClick={() => setOpen(false)}
                className="rounded-lg bg-slate-800 px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-slate-700"
              >
                Schließen
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
