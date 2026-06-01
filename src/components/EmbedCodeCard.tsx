// Karte „Website-Einbettung": zeigt den fertigen iFrame-Einbettcode für den
// konfigurierten Liveticker (passend zum gewählten Verband) und bietet einen
// Copy-Button. Der Turnier-Key wird aus der konfigurierten `live_url`
// (…/live?t=<key>) gelesen; das Embed läuft über badhub.de/embed/live.php.
import { useState } from "react";
import { Check, Copy } from "lucide-react";

/** Liest den Turnier-Key (`t`) aus der öffentlichen Live-URL. */
function tournamentKeyFromLiveUrl(liveUrl: string): string | null {
  try {
    return new URL(liveUrl).searchParams.get("t");
  } catch {
    return null;
  }
}

/** Baut das einbettbare iFrame-Snippet (mit Auto-Höhe per postMessage). */
function buildSnippet(key: string): string {
  return [
    `<iframe src="https://badhub.de/embed/live.php?t=${key}"`,
    `        id="badhub-live" title="Liveticker"`,
    `        style="width:100%;border:none;min-height:300px" scrolling="no"></iframe>`,
    `<script>`,
    `window.addEventListener("message", function (e) {`,
    `  if (e.origin !== "https://badhub.de") return;`,
    `  var f = document.getElementById("badhub-live");`,
    `  if (e.data && e.data.badhubLiveHeight) f.style.height = e.data.badhubLiveHeight + "px";`,
    `});`,
    `</script>`,
  ].join("\n");
}

export function EmbedCodeCard({ liveUrl }: { liveUrl: string }) {
  const key = tournamentKeyFromLiveUrl(liveUrl);
  const [copied, setCopied] = useState(false);

  // Ohne auflösbaren Key (z. B. manuelles Setup ohne live_url) keine Karte.
  if (!key) return null;
  const snippet = buildSnippet(key);

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
        Diesen Code auf der Verbands-Website (z.&nbsp;B. WordPress) einfügen — der
        Liveticker erscheint dort als eingebettetes Widget und passt seine Höhe
        automatisch an.
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
