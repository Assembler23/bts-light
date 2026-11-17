// Update-Banner: Zuordnung Lage → Text → Knöpfe für die Geräte-Seiten
// (Spec docs/features/hotfix-ohne-neustart.md, Abschnitt „Banner und
// Wartungsseite"). Reine Funktionen, damit jede Zeile der Tabelle einen Test
// hat — die Komponente malt nur, was hier herauskommt.
//
// Beide Zeilen stehen hier: die Seiten-Zeile (Seiten-Paket) und die
// Kern-Zeile (Update der App selbst — per Wechsel ohne Installer, wo er
// möglich ist, sonst per Installer wie bisher).

/** Beschriftungen der Knöpfe. */
export const KNOPF = {
  uebernehmen: "Jetzt übernehmen",
  zurueck: "Zurück zum vorigen Stand",
  schliessen: "Schließen",
  wechseln: "Jetzt wechseln",
  neu_starten: "Jetzt neu starten",
  beim_beenden: "Beim Beenden einbauen",
  doch_nicht: "Doch nicht beim Beenden",
  verstanden: "Verstanden",
  kern_zurueck: "Zurück zur vorigen Version",
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

/** „Auf N Feldern laufen Spiele" bzw. die Einzahl. */
function felderText(n) {
  return n === 1 ? "Auf 1 Feld läuft ein Spiel" : `Auf ${n} Feldern laufen Spiele`;
}

/** Hinweis auf die Unterbrechung beim Installer-Weg (Neustart, rund 20 s). */
export function unterbrechungsHinweis(info) {
  if (!info.sync_running) return "Die App startet dafür kurz neu.";
  if (info.occupied_courts === 0) {
    return "Kein Spiel läuft – die Übertragung setzt nach dem Neustart von selbst wieder ein.";
  }
  return `${felderText(info.occupied_courts)} – der Neustart unterbricht etwa 20 s, die Tablets zählen weiter und die Übertragung setzt von selbst wieder ein.`;
}

/** Hinweis auf die Unterbrechung beim Wechsel ohne Installer (wenige Sekunden). */
export function wechselHinweis(info) {
  if (!info.sync_running) return "Der Wechsel dauert wenige Sekunden.";
  if (info.occupied_courts === 0) {
    return "Kein Spiel läuft – der Wechsel unterbricht wenige Sekunden, die Übertragung läuft von selbst weiter.";
  }
  return `${felderText(info.occupied_courts)} – der Wechsel unterbricht wenige Sekunden, die Übertragung läuft von selbst weiter.`;
}

/** Warum der Rückweg zur Vorversion gesperrt ist (ADR 0101). */
function rueckwegGesperrtText(info, vorher) {
  return `Zurück zu v${vorher} ist nicht möglich: v${info.app_version} speichert Daten in einer Form, die v${vorher} nicht lesen kann.`;
}

/**
 * Der Kern-Rückweg auf der Wartungsseite — dauerhaft, nicht nur direkt nach
 * dem Wechsel. `null`, wenn es weder eine Vorversion noch eine
 * zurückgenommene Version gibt.
 */
export function wartungKern(info) {
  const kern = info?.kern;
  if (!kern) return null;
  const vorher = kern.zurueck_version ?? null;
  const hinweise = [];
  if (vorher && kern.zurueck_erlaubt) {
    hinweise.push(`Vorher lief v${vorher}. ${wechselHinweis(info)}`);
    hinweise.push(
      `Einstellungen, die es erst seit v${info.app_version} gibt, gehen dabei verloren, und v${info.app_version} wird danach nicht erneut angeboten.`,
    );
  } else if (vorher) {
    hinweise.push(rueckwegGesperrtText(info, vorher));
  }
  for (const v of kern.zurueckgenommen ?? []) {
    hinweise.push(`v${v} wurde zurückgenommen und wird nicht erneut angeboten.`);
  }
  if (hinweise.length === 0) return null;
  return {
    version: vorher,
    erlaubt: Boolean(vorher && kern.zurueck_erlaubt),
    knopf: vorher ? `Zurück zu v${vorher}` : null,
    hinweise,
  };
}

/**
 * Die Kern-Zeile des Banners — oder `null`.
 *
 * Reihenfolge: Erst, was gerade passiert ist (die neue Version lief nicht an
 * / der Wechsel ist geglückt), dann, was ansteht. Der Wechsel ohne Installer
 * wird nur angeboten, wenn die neue Programmdatei geprüft bereitliegt und zu
 * genau der Version gehört, die der Updater nennt; sonst bleibt es beim
 * Installer-Weg.
 */
export function kernZeile(info) {
  if (!info) return null;
  const kern = info.kern ?? {};
  const version = info.version ? ` (v${info.version})` : "";

  // Ein laufender Wechsel (auch der Rückweg) geht allem vor — sonst stünde
  // nach dem Klick auf „Zurück" weiter die alte Meldung da.
  if (kern.tausch === "wechselt") {
    return {
      art: "wechselt",
      text: `Wechsel auf v${kern.version} läuft – bts-light ist gleich wieder da.`,
      zusatz: [],
      knoepfe: [],
    };
  }
  if (kern.meldung === "zurueckgefallen") {
    const laeuft = info.sync_running ? ", die Übertragung läuft" : "";
    return {
      art: "zurueckgefallen",
      text: `v${kern.meldung_version} ließ sich nicht starten. bts-light läuft wieder mit v${info.app_version}${laeuft}.`,
      zusatz: [],
      knoepfe: ["verstanden"],
    };
  }
  if (kern.meldung === "zurueckgenommen") {
    return {
      art: "zurueckgenommen",
      text: `v${kern.meldung_version} wurde zurückgenommen und wird nicht erneut angeboten.`,
      zusatz: [`bts-light läuft wieder mit v${info.app_version}.`],
      knoepfe: ["schliessen"],
    };
  }
  if (kern.meldung === "gewechselt") {
    const zeile = {
      art: "gewechselt",
      text: `bts-light läuft jetzt mit v${info.app_version}.`,
      zusatz: [],
      knoepfe: ["schliessen"],
    };
    const vorher = kern.zurueck_version;
    if (vorher) {
      zeile.knoepfe = ["kern_zurueck", "schliessen"];
      zeile.namen = { kern_zurueck: `Zurück zu v${vorher}` };
      if (!kern.zurueck_erlaubt) {
        zeile.zusatz = [rueckwegGesperrtText(info, vorher)];
        zeile.aus = ["kern_zurueck"];
      }
    }
    return zeile;
  }
  if (info.phase === "downloading") {
    return { art: "laedt", text: `Update${version} wird im Hintergrund geladen …`, zusatz: [], knoepfe: [] };
  }
  if (info.phase === "installing") {
    return {
      art: "wechselt",
      text: `Update${version} wird eingebaut – die App startet gleich neu.`,
      zusatz: [],
      knoepfe: [],
    };
  }
  if (info.phase !== "ready") return null;

  if (info.install_on_exit) {
    return {
      art: "bereit",
      text: `Update${version} liegt bereit.`,
      zusatz: ["Es wird beim Beenden von bts-light still eingebaut."],
      knoepfe: ["doch_nicht"],
    };
  }
  const tauschBereit = kern.tausch === "bereit" && kern.version === info.version;
  if (tauschBereit) {
    const zusatz = [wechselHinweis(info)];
    if (!kern.rueckweg) {
      zusatz.push(`Ein Zurück zu v${info.app_version} ist nach diesem Update nicht möglich.`);
    }
    return {
      art: "bereit",
      text: `Update${version} liegt bereit.`,
      zusatz,
      knoepfe: ["beim_beenden", "wechseln"],
    };
  }
  return {
    art: "bereit",
    text: `Update${version} liegt bereit.`,
    zusatz: [unterbrechungsHinweis(info)],
    knoepfe: ["beim_beenden", "neu_starten"],
  };
}
