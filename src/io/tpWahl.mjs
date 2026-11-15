/**
 * TP-Datei im Setup: Was geschieht mit einer gewählten oder gefundenen Datei?
 * (Spec `docs/features/tp-datei-finden.md`, ADR 0096/0097.)
 *
 * Reine Entscheidung, getrennt vom Formular: Dateidialog und Suche in BTP
 * laufen durch dieselbe Regel. Geprüft von `scripts/test-tp-wahl.mjs`.
 */

/** Grund, warum eine Datei nicht taugt — oder `null`, wenn sie erkannt ist. */
export function tpInfoText(info) {
  switch (info.art) {
    case "erkannt":
      return null;
    case "zu_gross":
      return "Die Datei ist größer als 32 MB — zu groß für den Upload.";
    case "nicht_gefunden":
      return "Datei nicht gefunden oder nicht lesbar — im Setup neu wählen.";
    default:
      return "Das ist keine TP-Datei (Turnierdatenbank von BTP).";
  }
}

/**
 * Entscheidet über eine Datei.
 *
 * - `eingetragen`: kanonische Kennung aus „1 · Liveticker-Ziel", "" wenn dort
 *   keine gültige steht.
 * - `feldLeer`: Das Feld ist wirklich leer (nicht bloß ungültig gefüllt).
 * - `quelle`: "dialog" (von Hand gewählt) oder "suche" (in BTP gefunden).
 *
 * Ergebnis:
 * - `{ art: "fehler", text }` — nichts übernehmen.
 * - `{ art: "frage", dateiGuid, eingetragen }` — erst nach Bestätigung.
 *   `dateiGuid === ""` heißt: Die Datei nennt keine Kennung.
 * - `{ art: "uebernehmen", bindung, kennungNeu }` — still übernehmen;
 *   `kennungNeu` ist die Kennung, die ins leere Feld gehört, sonst `null`.
 */
export function tpWahl({ info, name, eingetragen, feldLeer, quelle }) {
  const grund = tpInfoText(info);
  if (grund !== null) return { art: "fehler", text: `„${name}“: ${grund}` };
  const dateiGuid = info.guid ?? "";
  if (eingetragen === "") {
    if (dateiGuid === "") {
      // Leer, mehrdeutig oder ohne Anker — die Datei allein reicht nicht.
      return {
        art: "fehler",
        text: `„${name}“ nennt keine eindeutige Turnier-Kennung — erst unter „1 · Liveticker-Ziel“ die Turnier-Adresse eintragen, dann die Datei wählen.`,
      };
    }
    // Etwas steht im Feld, ist aber keine Kennung: nicht still ersetzen.
    if (!feldLeer) return { art: "frage", dateiGuid, eingetragen: "" };
    return { art: "uebernehmen", bindung: dateiGuid, kennungNeu: dateiGuid };
  }
  // Andere Kennung als eingetragen: Die Kennung steuert auch Liveticker,
  // Check-In und Aushang — nie still umlenken (ADR 0097).
  if (dateiGuid !== "" && dateiGuid !== eingetragen) {
    return { art: "frage", dateiGuid, eingetragen };
  }
  // Datei ohne Kennung: Von Hand gewählt gilt die Wahl (der Server
  // entscheidet), von der Suche gefunden wird gefragt — niemand hat diese
  // Datei ausgesucht.
  if (dateiGuid === "" && quelle === "suche") {
    return { art: "frage", dateiGuid: "", eingetragen };
  }
  return { art: "uebernehmen", bindung: eingetragen, kennungNeu: null };
}

/**
 * Meldung im Setup, wenn die Suche in BTP nichts liefert — oder `null`, wenn
 * es nichts zu sagen gibt (Fund, Suche läuft schon, kein Windows).
 */
export function tpSucheText(suche) {
  switch (suche.art) {
    case "btp_nicht_gestartet":
      return "BTP läuft nicht auf diesem PC.";
    case "kein_turnier_offen":
      return "In BTP ist kein Turnier geöffnet.";
    case "kein_zugriff":
      return "BTP läuft als Administrator — bitte die Datei von Hand wählen.";
    case "haenger":
      return "BTP hat nicht geantwortet — bitte die Datei von Hand wählen.";
    case "mehrere":
      return "Mehrere Turnierdateien offen — bitte die Datei von Hand wählen.";
    case "netzlaufwerk":
      return `„${suche.name}“ liegt auf einem Netzlaufwerk — bitte die Datei von Hand wählen.`;
    case "nicht_lokal":
      return "BTP läuft auf einem anderen PC — bitte die Datei von Hand wählen.";
    case "vorpruefung":
      return `„${suche.name}“: ${tpInfoText(suche.info)}`;
    default:
      return null;
  }
}
