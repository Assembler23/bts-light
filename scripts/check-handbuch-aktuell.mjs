#!/usr/bin/env node
// Meldet, wenn eine Änderung die Endnutzer-Sicht anfasst, das Handbuch aber
// nicht mitgewachsen ist.
//
// WARUM: Ein Handbuch meldet sich nicht, wenn es veraltet. Beim Erstellen der
// Kapitel (05./06.09.2026) fanden die Gegenprüfungen über dreißig Sachfehler —
// und das bei Texten, die am selben Tag geschrieben wurden. Ein halbes Jahr
// Weiterentwicklung ohne Nachziehen wäre schlimmer.
//
// BLOCKIERT NICHTS. Kein Required-Check, kein roter Merge. Ein Refactor ohne
// Nutzerwirkung soll nicht an einer Doku-Formalie hängenbleiben, und wer eine
// Umgehung braucht, gewöhnt sie sich an. Der Workflow schreibt eine sichtbare
// Anmerkung an den Lauf — mehr nicht. Die scharfe Prüfung ist
// scripts/test-handbuch-fakten.mjs, die wird rot.
//
// Aufruf:  node scripts/check-handbuch-aktuell.mjs <datei> [<datei> …]
//          node scripts/check-handbuch-aktuell.mjs --git origin/main
// Exit:    immer 0 (siehe oben). Ausgabe: Klartext + GitHub-Anmerkungen.

import { execFileSync } from "node:child_process";

/**
 * Was ist "Endnutzer-sichtbar"? Nur Pfade, deren Änderung ein Turnierleiter
 * oder ein Schiedsrichter merken KANN. Bewusst eng: Ein Wächter, der bei
 * jedem Commit anschlägt, wird nach einer Woche überlesen.
 *
 * Je Eintrag steht dabei, WELCHE Kapitel erfahrungsgemäß betroffen sind —
 * damit die Meldung nicht nur "Doku prüfen" sagt, sondern wohin man schaut.
 */
export const BEREICHE = [
  {
    muster: /^src\/pages\/SetupWizard\.tsx$/,
    was: "Einstellungen / Einrichtungs-Assistent",
    kapitel: ["docs/einstellungen.md", "docs/erste-schritte.md"],
  },
  {
    muster: /^src-tauri\/src\/config\.rs$/,
    was: "Konfiguration (Standardwerte!)",
    kapitel: ["docs/einstellungen.md"],
  },
  {
    muster: /^src-tauri\/assets\/tablet\.html$/,
    was: "Zähltablett",
    kapitel: ["docs/tablet-bedienen.md", "docs/tablet.md", "docs/umpire-mode.md"],
  },
  {
    muster: /^src-tauri\/assets\/(monitor|combo|overview|preparation|winners|ad|tafel|anzeige|lobby|tv)\.html$/,
    was: "Anzeigen in der Halle",
    kapitel: ["docs/court-monitor.md", "docs/oberflaechen.md"],
  },
  {
    muster: /^src-tauri\/assets\/tl\.html$/,
    was: "Turnierleitungs-Oberfläche",
    kapitel: ["docs/turnierleitung-web.md"],
  },
  {
    muster: /^src\/components\/SideNav\.tsx$/,
    was: "Menü der App",
    kapitel: ["docs/oberflaechen.md"],
  },
  {
    muster: /^src\/pages\/(?!SetupWizard)/,
    was: "ein Bildschirm der App",
    kapitel: ["docs/oberflaechen.md"],
  },
  {
    muster: /^src-tauri\/src\/tablet\/server\.rs$/,
    was: "Adressen/Routen der Hallen-Seiten",
    kapitel: ["docs/oberflaechen.md"],
  },
  {
    muster: /^src-tauri\/installer\//,
    was: "Installer (Firewall!)",
    kapitel: ["docs/erste-schritte.md"],
  },
  {
    muster: /^src-tauri\/src\/update\.rs$/,
    was: "Update-Ablauf",
    kapitel: ["docs/erste-schritte.md"],
  },
];

/** Zählt als "Handbuch mitgewachsen": jede Änderung an einer docs/-Datei. */
export function handbuchAngefasst(dateien) {
  return dateien.some((d) => d.startsWith("docs/") || d === "README.md");
}

/** Betroffene Bereiche der Änderung — leer heißt: nichts Endnutzer-Sichtbares. */
export function betroffen(dateien) {
  const treffer = [];
  for (const b of BEREICHE) {
    const passend = dateien.filter((d) => b.muster.test(d));
    if (passend.length) treffer.push({ ...b, dateien: passend });
  }
  return treffer;
}

/** Die Meldung — als reiner Text, damit sie testbar ist. */
export function bericht(dateien) {
  const bereiche = betroffen(dateien);
  if (bereiche.length === 0) return null;
  if (handbuchAngefasst(dateien)) return null;

  const zeilen = [
    "Diese Änderung fasst die Endnutzer-Sicht an, aber kein Kapitel des Handbuchs:",
    "",
  ];
  for (const b of bereiche) {
    zeilen.push(`• ${b.was} — ${b.dateien.join(", ")}`);
    zeilen.push(`  Sieh nach in: ${b.kapitel.join(" · ")}`);
  }
  zeilen.push("");
  zeilen.push("Wenn sich für Nutzer nichts ändert, ist das in Ordnung — dann ignorier");
  zeilen.push("diesen Hinweis. Er blockiert nichts.");
  return zeilen.join("\n");
}

// ── Kommandozeile ─────────────────────────────────────────────────────────
if (import.meta.url === `file://${process.argv[1]}`) {
  let dateien;
  const i = process.argv.indexOf("--git");
  if (i >= 0) {
    const basis = process.argv[i + 1] || "origin/main";
    dateien = execFileSync("git", ["diff", "--name-only", `${basis}...HEAD`], {
      encoding: "utf8",
    })
      .split("\n")
      .filter(Boolean);
  } else {
    dateien = process.argv.slice(2).filter((a) => !a.startsWith("--"));
  }

  const text = bericht(dateien);
  if (text) {
    console.log(text);
    // Als GitHub-Anmerkung, damit sie im Lauf sichtbar ist — ::warning und
    // nicht ::error, weil dieser Check bewusst nichts blockiert.
    const einzeilig = text.replace(/\n/g, "%0A");
    console.log(`::warning title=Handbuch prüfen::${einzeilig}`);
  } else {
    console.log("Handbuch: nichts zu tun.");
  }
  process.exit(0);
}
