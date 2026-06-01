// Kompakter Button „Einbettcode kopieren" für einen Verband — kopiert den
// fertigen <script>-Einzeiler der „Jetzt live"-Box in die Zwischenablage.
// Wird im Setup-Wizard hinter jeder LV-Preset-Karte angezeigt.
import { useState } from "react";
import { Check, Code2 } from "lucide-react";
import { buildBadgeSnippet, tournamentKeyFromLiveUrl } from "../embedSnippet";

export function CopyBadgeButton({ liveUrl }: { liveUrl: string }) {
  const key = tournamentKeyFromLiveUrl(liveUrl);
  const [copied, setCopied] = useState(false);

  // Ohne auflösbaren Key (z. B. manuell) keinen Button zeigen.
  if (!key) return null;

  async function copy() {
    try {
      await navigator.clipboard.writeText(buildBadgeSnippet(key as string));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* Clipboard nicht verfügbar – still ignorieren */
    }
  }

  return (
    <button
      type="button"
      onClick={copy}
      title="Einbettcode für die Verbands-Website kopieren"
      className="flex shrink-0 flex-col items-center justify-center gap-1 rounded-xl border
                 border-slate-200 px-3 text-xs font-medium text-slate-600
                 transition-colors hover:bg-slate-50"
    >
      {copied ? (
        <Check size={16} strokeWidth={2} className="text-emerald-600" />
      ) : (
        <Code2 size={16} strokeWidth={2} />
      )}
      {copied ? "Kopiert" : "Code"}
    </button>
  );
}
