// Karte „Website-Einbettung": zeigt den fertigen Einbettcode (kompakte
// „Jetzt live"-Box) für den konfigurierten Verband und bietet einen Copy-Button.
// Der Turnier-Key kommt aus der konfigurierten `live_url` (…/live?t=<key>);
// das Widget läuft über badhub.de/embed/badge.php.
import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { buildBadgeSnippet, tournamentKeyFromLiveUrl } from "../embedSnippet";

export function EmbedCodeCard({ liveUrl }: { liveUrl: string }) {
  const key = tournamentKeyFromLiveUrl(liveUrl);
  const [copied, setCopied] = useState(false);

  // Ohne auflösbaren Key (z. B. manuelles Setup ohne live_url) keine Karte.
  if (!key) return null;
  const snippet = buildBadgeSnippet(key);

  async function copy() {
    try {
      await navigator.clipboard.writeText(snippet);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* Clipboard nicht verfügbar – still ignorieren */
    }
  }

  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-semibold text-slate-700">Website-Einbettung</h2>
      <p className="text-xs text-slate-500">
        Diesen Code auf der Verbands-Website (z.&nbsp;B. WordPress) einfügen — es
        erscheint eine kompakte „Jetzt live"-Box, sobald ein Turnier läuft; ein
        Klick darauf führt zum Liveticker. Läuft nichts, bleibt die Box unsichtbar.
      </p>
      <pre className="overflow-x-auto rounded-lg border border-slate-200 bg-slate-50 p-3 text-xs leading-relaxed text-slate-700">
        <code>{snippet}</code>
      </pre>
      <div>
        <button
          onClick={copy}
          className="inline-flex items-center gap-2 rounded-lg bg-slate-100 px-3.5 py-2
                     text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
        >
          {copied ? <Check size={16} strokeWidth={2} /> : <Copy size={16} strokeWidth={2} />}
          {copied ? "Kopiert" : "Code kopieren"}
        </button>
      </div>
    </section>
  );
}
