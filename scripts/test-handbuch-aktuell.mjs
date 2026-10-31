#!/usr/bin/env node
// Die Regel hinter dem "Handbuch prüfen?"-Hinweis.
//
// WARUM GETESTET: Ein Wächter, der zu oft anschlägt, wird nach einer Woche
// überlesen — dann ist er schlimmer als keiner, weil man sich in Sicherheit
// wiegt. Ein Wächter, der zu selten anschlägt, ist wirkungslos. Beide Grenzen
// sind hier festgehalten, damit sie nicht unbemerkt verrutschen.
//
// Aufruf:  node scripts/test-handbuch-aktuell.mjs
// Exit:    0 = ok, 1 = mind. eine Prüfung fehlgeschlagen.

import { bericht, betroffen, handbuchAngefasst, BEREICHE } from "./check-handbuch-aktuell.mjs";

let fehler = 0;
const pruefe = (ok, text) => {
  console.log(`  ${ok ? "✓" : "✗"} ${text}`);
  if (!ok) fehler++;
};

console.log("\nSchlägt an, wo es soll");
pruefe(bericht(["src-tauri/src/config.rs"]) !== null, "Standardwerte geändert, Doku nicht");
pruefe(bericht(["src/pages/SetupWizard.tsx"]) !== null, "Einstellungsseite geändert, Doku nicht");
pruefe(bericht(["src-tauri/assets/tablet.html"]) !== null, "Zähltablett geändert, Doku nicht");
pruefe(bericht(["src-tauri/assets/tafel.html"]) !== null, "eine Hallen-Anzeige geändert, Doku nicht");
pruefe(bericht(["src/components/SideNav.tsx"]) !== null, "Menü geändert, Doku nicht");
pruefe(bericht(["src-tauri/installer/firewall-hooks.nsh"]) !== null, "Installer geändert, Doku nicht");
pruefe(bericht(["src-tauri/src/update.rs"]) !== null, "Update-Ablauf geändert, Doku nicht");
pruefe(bericht(["src-tauri/src/tablet/server.rs"]) !== null, "Routen geändert, Doku nicht");

console.log("\nSchweigt, wo es soll");
pruefe(bericht(["src-tauri/src/config.rs", "docs/einstellungen.md"]) === null,
  "Doku mitgeändert ⇒ still");
pruefe(bericht(["src-tauri/src/config.rs", "README.md"]) === null,
  "README zählt als Handbuch-Kapitel (es ist eines)");
pruefe(bericht(["src-tauri/src/btp/proto.rs"]) === null,
  "Protokoll-Innenleben ist für Nutzer unsichtbar");
pruefe(bericht(["relay/src/main.rs"]) === null, "Relay-Innenleben ⇒ still");
pruefe(bericht(["scripts/test-serving.mjs"]) === null, "ein Test ⇒ still");
pruefe(bericht([".github/workflows/ci.yml"]) === null, "CI-Umbau ⇒ still");
pruefe(bericht(["Cargo.lock", "package-lock.json"]) === null, "Abhängigkeits-Update ⇒ still");
pruefe(bericht([]) === null, "leere Änderung ⇒ still");

console.log("\nDie Meldung ist brauchbar");
const text = bericht(["src-tauri/src/config.rs"]);
pruefe(/docs\/einstellungen\.md/.test(text), "nennt das Kapitel, in das man schauen muss");
pruefe(/config\.rs/.test(text), "nennt die geänderte Datei");
pruefe(/blockiert nichts/.test(text), "sagt ausdrücklich, dass sie nichts blockiert");

console.log("\nDie Regel selbst");
pruefe(BEREICHE.every((b) => b.kapitel.length > 0),
  "jeder überwachte Bereich nennt mindestens ein Kapitel");
pruefe(BEREICHE.every((b) => b.was && b.was.length > 3),
  "jeder Bereich hat eine Klartext-Beschreibung");
pruefe(betroffen(["src/pages/Dashboard.tsx"]).length === 1,
  "eine App-Seite trifft genau einen Bereich (keine Doppelmeldung)");
pruefe(handbuchAngefasst(["docs/irgendwas.md"]), "jede docs/-Datei zählt");
pruefe(!handbuchAngefasst(["src/main.tsx"]), "Quellcode zählt nicht als Doku");

// SetupWizard darf NICHT zusaetzlich als "ein Bildschirm der App" gemeldet
// werden — sonst steht dieselbe Aenderung zweimal in der Meldung.
pruefe(
  betroffen(["src/pages/SetupWizard.tsx"]).length === 1,
  "der Einrichtungs-Assistent wird nur einmal gemeldet"
);

console.log(fehler === 0 ? "\nOK" : `\n${fehler} fehlgeschlagen`);
process.exit(fehler === 0 ? 0 : 1);
