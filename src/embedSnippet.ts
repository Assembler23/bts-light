// Gemeinsame Helfer für den Website-Einbettcode (kompakte „Jetzt live"-Box).
// Genutzt von der Dashboard-Karte und den Preset-Buttons im Setup-Wizard.

/** Liest den Turnier-Key (`t`) aus der öffentlichen Live-URL (…/live?t=<key>). */
export function tournamentKeyFromLiveUrl(liveUrl: string): string | null {
  try {
    return new URL(liveUrl).searchParams.get("t");
  } catch {
    return null;
  }
}

/**
 * Baut den einzubettenden Einzeiler für die „Jetzt live"-Box. Bewusst EIN
 * <script>-Tag (kein mehrzeiliges Inline-JS) — WordPress zerschießt sonst den
 * Code (Smart-Quotes / automatische <br>). Das Skript hängt die Box selbst ein
 * und blendet sie aus, wenn gerade nichts live ist.
 */
export function buildBadgeSnippet(key: string): string {
  return `<script src="https://badhub.de/embed/badge.php" data-key="${key}"></script>`;
}
