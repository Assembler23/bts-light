#!/usr/bin/env node
/**
 * Feldtest „Hotfix ohne Neustart": liest die Lücken der Übertragung aus den
 * Logdateien von bts-light und wertet sie gegen Erfolgskriterium 2 der Spec
 * aus (docs/features/hotfix-ohne-neustart.md).
 *
 * Aufruf:  node scripts/luecken-auswerten.mjs [--seit 2026-11-17] <Logdatei oder Log-Ordner> …
 * Windows: node scripts/luecken-auswerten.mjs "%LOCALAPPDATA%\de.badhub.btslight\logs"
 *
 * `--seit` lässt ältere Zeilen weg — Kriterium 2 verlangt Läufe „im selben
 * Verfahren", also die des Feldtests, nicht irgendwelche früheren Updates.
 *
 * Jede Lücke steht in der Zeile „Wiederanlauf nach Update: Tablet-Port wieder
 * offen, Lücke N ms". Welcher Weg sie verursacht hat, sagt die letzte Zeile
 * davor, mit der die Vorversion ihren Abgang ankündigt:
 *   „Kernwechsel auf vX: Wächter gestartet"   → Wechsel (bzw. Rückweg, wenn
 *                                               davor „Rückweg auf vX:
 *                                               Programmdatei getauscht" kam)
 *   „Update auf vX wird eingebaut"            → Installer
 *
 * Ein Tausch zählt nur, wenn die Zielversion danach ihre Ankunft gemeldet hat
 * („Kernwechsel: Ankunft von vX gemeldet"). Fehlt die Zeile, hat der Wächter
 * zurückgerollt: Die Lücke gehört dann der wieder gestarteten alten Version
 * und steht als „Rückfall" für sich — sie darf den Median nicht schönen.
 *
 * Rechnen und Urteilen sind reine Funktionen (Test:
 * scripts/test-luecken-auswerten.mjs).
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

/** Erfolgskriterium 2 der Spec. */
export const KRITERIUM = { laeufe: 10, medianMs: 3000, maxMs: 5000, anteilInstaller: 1 / 3 };

const ZEIT = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)/;
const LUECKE = /Wiederanlauf nach Update: Tablet-Port wieder offen, Lücke (\d+) ms/;
const TAUSCH = /Kernwechsel auf v(\S+): Wächter gestartet/;
const RUECKWEG = /Rückweg auf v(\S+): Programmdatei getauscht/;
const INSTALLER = /Update auf v(\S+) wird eingebaut/;
const ANKUNFT = /Kernwechsel: Ankunft von v(\S+) gemeldet/;

/**
 * Sucht die Lücken in Logzeilen (beliebige Reihenfolge, mehrere Dateien).
 * @param {string[]} zeilen
 * @param {{seit?: string}} [opt] `seit`: ISO-Zeit oder -Datum; ältere Zeilen entfallen
 * @returns {{zeit: string, weg: "wechsel"|"rueckweg"|"rueckfall"|"installer"|"unbekannt", version: string|null, ms: number}[]}
 */
export function lueckenAus(zeilen, opt = {}) {
  const mitZeit = [];
  for (const zeile of zeilen) {
    const m = ZEIT.exec(zeile);
    if (m && (!opt.seit || m[1] >= opt.seit)) mitZeit.push({ zeit: m[1], zeile });
  }
  // Zeitstempel im ISO-Format sortieren als Text richtig.
  mitZeit.sort((a, b) => (a.zeit < b.zeit ? -1 : a.zeit > b.zeit ? 1 : 0));

  const luecken = [];
  let abgang = null; // der zuletzt angekündigte Abgang
  let rueckwegOffen = null; // „Rückweg … getauscht" wartet auf den Wächter
  // Tausch-Lücken, deren Zielversion sich noch nicht gemeldet hat. Die
  // Ankunft kommt mal vor, mal nach der Lücken-Zeile (Port offen vs. erstes
  // Lebenszeichen des Fensters) — deshalb wird sie dem ganzen Abschnitt bis
  // zum nächsten Abgang zugerechnet.
  let abschnitt = null; // { version, angekommen, luecke }
  const abschliessen = () => {
    if (abschnitt?.luecke && !abschnitt.angekommen) abschnitt.luecke.weg = "rueckfall";
    abschnitt = null;
  };
  for (const { zeit, zeile } of mitZeit) {
    let m;
    if ((m = RUECKWEG.exec(zeile))) {
      rueckwegOffen = m[1];
    } else if ((m = TAUSCH.exec(zeile))) {
      abschliessen();
      abgang =
        rueckwegOffen === m[1]
          ? { weg: "rueckweg", version: m[1] }
          : { weg: "wechsel", version: m[1] };
      abschnitt = { version: m[1], angekommen: false, luecke: null };
      rueckwegOffen = null;
    } else if ((m = INSTALLER.exec(zeile))) {
      abschliessen();
      abgang = { weg: "installer", version: m[1] };
      rueckwegOffen = null;
    } else if ((m = ANKUNFT.exec(zeile))) {
      if (abschnitt && abschnitt.version === m[1]) abschnitt.angekommen = true;
    } else if ((m = LUECKE.exec(zeile))) {
      const luecke = {
        zeit,
        weg: abgang?.weg ?? "unbekannt",
        version: abgang?.version ?? null,
        ms: Number(m[1]),
      };
      luecken.push(luecke);
      if (abschnitt && abgang && abgang.weg !== "installer" && !abschnitt.luecke) {
        abschnitt.luecke = luecke;
      }
      // Jeder Abgang erklärt genau eine Lücke.
      abgang = null;
      rueckwegOffen = null;
    }
  }
  abschliessen();
  return luecken;
}

/** Median einer Zahlenliste; `null` bei leerer Liste. */
export function median(werte) {
  if (werte.length === 0) return null;
  const s = [...werte].sort((a, b) => a - b);
  const mitte = Math.floor(s.length / 2);
  return s.length % 2 === 1 ? s[mitte] : Math.round((s[mitte - 1] + s[mitte]) / 2);
}

/**
 * Kennzahlen je Weg und das Urteil zu Kriterium 2. Wechsel und Rückweg sind
 * derselbe Ablauf und zählen gemeinsam als „Tausch".
 */
export function auswerten(luecken) {
  const von = (wege) => luecken.filter((l) => wege.includes(l.weg)).map((l) => l.ms);
  const kennzahl = (werte) => ({
    laeufe: werte.length,
    medianMs: median(werte),
    maxMs: werte.length ? Math.max(...werte) : null,
  });
  const tausch = kennzahl(von(["wechsel", "rueckweg"]));
  const installer = kennzahl(von(["installer"]));
  const unbekannt = kennzahl(von(["unbekannt"]));
  const rueckfall = kennzahl(von(["rueckfall"]));

  const offen = [];
  const verletzt = [];
  if (tausch.laeufe < KRITERIUM.laeufe) {
    offen.push(`noch ${KRITERIUM.laeufe - tausch.laeufe} Tausch-Läufe bis ${KRITERIUM.laeufe}`);
  }
  if (installer.laeufe < KRITERIUM.laeufe) {
    offen.push(`noch ${KRITERIUM.laeufe - installer.laeufe} Installer-Läufe bis ${KRITERIUM.laeufe}`);
  }
  // Eine Lücke, deren Weg sich nicht erkennen lässt, könnte ein Tausch
  // gewesen sein — solange sie nicht von Hand zugeordnet ist, gibt es kein
  // „erfüllt".
  if (unbekannt.laeufe > 0) {
    offen.push(`${unbekannt.laeufe} Lücke(n) ohne erkennbaren Weg – von Hand zuordnen oder mit --seit ausblenden`);
  }
  if (tausch.medianMs != null && tausch.medianMs > KRITERIUM.medianMs) {
    verletzt.push(`Median ${tausch.medianMs} ms liegt über ${KRITERIUM.medianMs} ms`);
  }
  if (tausch.maxMs != null && tausch.maxMs > KRITERIUM.maxMs) {
    verletzt.push(`Maximum ${tausch.maxMs} ms liegt über ${KRITERIUM.maxMs} ms`);
  }
  if (tausch.medianMs != null && installer.medianMs != null) {
    const grenze = Math.floor(installer.medianMs * KRITERIUM.anteilInstaller);
    if (tausch.medianMs > grenze) {
      verletzt.push(
        `Median ${tausch.medianMs} ms liegt über einem Drittel des Installer-Wegs (${grenze} ms)`,
      );
    }
  }
  // „Erfüllt" erst, wenn nichts verletzt ist UND genug Läufe vorliegen.
  const urteil = verletzt.length > 0 ? "verfehlt" : offen.length > 0 ? "offen" : "erfuellt";
  return { tausch, installer, unbekannt, rueckfall, offen, verletzt, urteil };
}

const WEG_TEXT = {
  wechsel: "Wechsel",
  rueckweg: "Rückweg",
  rueckfall: "Rückfall (zählt nicht)",
  installer: "Installer",
  unbekannt: "unbekannt",
};

/** Der Bericht als Text (Markdown-Tabelle zum Einfügen ins Protokoll). */
export function bericht(luecken) {
  // Die Drittel-Regel vergleicht Median mit Median — die Spec nennt die
  // Kennzahl nicht; der Median ist die, die sie für die Lücke selbst nennt.
  const a = auswerten(luecken);
  const z = [];
  z.push("| Zeit (UTC) | Weg | auf Version | Lücke |");
  z.push("|---|---|---|---|");
  for (const l of luecken) {
    z.push(`| ${l.zeit.slice(0, 19).replace("T", " ")} | ${WEG_TEXT[l.weg]} | ${l.version ?? "–"} | ${l.ms} ms |`);
  }
  if (luecken.length === 0) z.push("| – | – | – | – |");
  z.push("");
  const zeile = (name, k) =>
    `${name}: ${k.laeufe} Läufe` +
    (k.laeufe ? `, Median ${k.medianMs} ms, Maximum ${k.maxMs} ms` : "");
  z.push(zeile("Tausch (Wechsel + Rückweg)", a.tausch));
  z.push(zeile("Installer", a.installer));
  if (a.rueckfall.laeufe) z.push(zeile("Rückfälle (nicht gewertet)", a.rueckfall));
  if (a.unbekannt.laeufe) z.push(zeile("Ohne erkennbaren Weg", a.unbekannt));
  z.push("");
  const URTEIL = {
    erfuellt: "Kriterium 2 ERFÜLLT",
    verfehlt: "Kriterium 2 VERFEHLT",
    offen: "Kriterium 2 noch OFFEN",
  };
  z.push(`${URTEIL[a.urteil]} (Median ≤ ${KRITERIUM.medianMs} ms, Maximum ≤ ${KRITERIUM.maxMs} ms, höchstens ein Drittel des Installer-Wegs, je ${KRITERIUM.laeufe} Läufe).`);
  for (const v of a.verletzt) z.push(`- ${v}`);
  for (const o of a.offen) z.push(`- ${o}`);
  return z.join("\n");
}

/** Alle Zeilen aus Dateien und Ordnern (im Ordner: `bts-light.log*`). */
function zeilenAus(pfade) {
  const zeilen = [];
  for (const pfad of pfade) {
    const dateien = statSync(pfad).isDirectory()
      ? readdirSync(pfad)
          .filter((n) => n.startsWith("bts-light.log"))
          .map((n) => join(pfad, n))
      : [pfad];
    for (const datei of dateien) zeilen.push(...readFileSync(datei, "utf8").split(/\r?\n/));
  }
  return zeilen;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const pfade = process.argv.slice(2);
  let seit;
  const i = pfade.indexOf("--seit");
  if (i >= 0) [, seit] = pfade.splice(i, 2);
  if (pfade.length === 0 || (i >= 0 && !/^\d{4}-\d{2}-\d{2}/.test(seit ?? ""))) {
    console.error(
      "Aufruf: node scripts/luecken-auswerten.mjs [--seit JJJJ-MM-TT] <Logdatei oder Log-Ordner> …",
    );
    process.exit(2);
  }
  let zeilen;
  try {
    zeilen = zeilenAus(pfade);
  } catch (e) {
    console.error(`Log nicht lesbar: ${e.message}`);
    process.exit(2);
  }
  console.log(bericht(lueckenAus(zeilen, { seit })));
}
