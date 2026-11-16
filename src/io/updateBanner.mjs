// Update-Banner: Zuordnung Lage → Text → Knöpfe für die Geräte-Seiten
// (Spec docs/features/hotfix-ohne-neustart.md, Abschnitt „Banner und
// Wartungsseite"). Reine Funktionen, damit jede Zeile der Tabelle einen Test
// hat — die Komponente malt nur, was hier herauskommt.
//
// Der Kern-Teil des Banners (Update der App selbst) steht noch in
// `components/UpdateBanner.tsx` und zieht mit dem Kernwechsel hierher.

/** Beschriftungen der Knöpfe. */
export const KNOPF = {
  uebernehmen: "Jetzt übernehmen",
  zurueck: "Zurück zum vorigen Stand",
  schliessen: "Schließen",
};

const ZUSATZ_CLOUD =
  "Gilt für Geräte im Hallennetz. Geräte über die Cloud bekommen ihre Seiten vom Server.";
const ZUSATZ_VERBUND = "Weitere PCs (andere Halle) werden dort einzeln aktualisiert.";

function zusaetze(seiten) {
  const out = [];
  if (seiten.reichweite === "lan+cloud") out.push(ZUSATZ_CLOUD);
  if (seiten.verbund) out.push(ZUSATZ_VERBUND);
  return out;
}

/**
 * Die Seiten-Zeile des Banners — oder `null`, wenn es nichts zu sagen gibt.
 *
 * Eine Meldung nach einer Aktion (übernommen, zurückgenommen) hat Vorrang vor
 * einem neuen Angebot: Wer gerade geklickt hat, soll zuerst lesen, was
 * passiert ist und wie er zurückkommt.
 */
export function seitenZeile(seiten) {
  if (!seiten) return null;
  if (seiten.meldung === "uebernommen") {
    return {
      art: "uebernommen",
      text:
        "Seiten aktualisiert. Tablets laden neu, sobald kein Spiel läuft – sonst zeigt das Tablet einen Hinweis. " +
        "Monitore über „Neu laden“. Die Turnierleitungs-Seite bitte von Hand neu laden.",
      zusatz: zusaetze(seiten),
      // Ohne Stand dahinter (nach einem zweiten Rückweg) kein Zurück-Knopf.
      knoepfe: seiten.rueckweg ? ["zurueck", "schliessen"] : ["schliessen"],
    };
  }
  if (seiten.meldung === "zurueckgenommen") {
    const welcher = seiten.meldung_folge != null ? `Seiten-Stand ${seiten.meldung_folge}` : "Der Seiten-Stand";
    return {
      art: "zurueckgenommen",
      text: `${welcher} wurde zurückgenommen und wird nicht erneut angeboten.`,
      zusatz: [],
      knoepfe: ["schliessen"],
    };
  }
  // Im reinen Cloud-Betrieb liefert dieser PC keine Seiten aus: kein Angebot.
  if (seiten.reichweite === "keine" || seiten.bereit_folge == null) return null;
  return {
    art: "angebot",
    // Mit der Nummer: So ist erkennbar, WELCHER Stand angeboten wird — etwa
    // einer, den man an diesem PC schon einmal zurückgenommen hat.
    text: `Verbesserung für Tablets und Anzeigen liegt bereit (Seiten-Stand ${seiten.bereit_folge}) – ohne Unterbrechung.`,
    zusatz: zusaetze(seiten),
    knoepfe: ["uebernehmen"],
  };
}

/**
 * Was die Wartungsseite dauerhaft zeigt: den Stand, Hinweise und ob der
 * Rückweg-Knopf da ist.
 */
export function wartungSeiten(info) {
  if (!info?.seiten) return null;
  const s = info.seiten;
  const hinweise = [];
  if (s.reichweite === "keine") hinweise.push("Im Cloud-Betrieb kommen die Seiten vom Server.");
  if (s.abgelehnt) hinweise.push(`Paket abgelehnt, nichts geändert (${s.abgelehnt}).`);
  if (s.bereit_folge != null && s.reichweite !== "keine") {
    hinweise.push(`Seiten-Stand ${s.bereit_folge} liegt bereit – Übernahme über das Banner oben.`);
  }
  return {
    stand: `App v${info.app_version} · Seiten-Stand ${s.aktive_folge}${s.aktive_folge === 0 ? " (eingebaut)" : ""}`,
    hinweise,
    rueckweg: s.rueckweg,
  };
}
