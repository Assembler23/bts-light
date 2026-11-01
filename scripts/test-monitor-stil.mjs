// Testet die Monitor-Stile (src/io/monitorStil.mjs, Spec
// monitor-stile-und-stream, ADR 0068) und die Inline-Kopie in monitor.html.
// Hier entscheidet sich, welcher Wert je in `data-stil` (und damit in CSS)
// landet und ob ein Feldwechsel im Panel den Stil behält.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { STILE, stilAusWert, wirksamerStil, zielMitStil, kurzname, namenPlaetze } from "../src/io/monitorStil.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Allowlist ────────────────────────────────────────────────────────────
ok("sechs Stile", STILE, ["standard", "international", "riesen", "spieler", "hell", "stand"]);
for (const s of STILE) ok(`bekannter Stil ${s} bleibt`, stilAusWert(s), s);
ok("unbekannt → standard", stilAusWert("zukunft"), "standard");
ok("Groß-/Kleinschreibung zählt", stilAusWert("Hell"), "standard");
ok("CSS-Einschleusung → standard", stilAusWert('hell"] , body{'), "standard");
ok("null → standard", stilAusWert(null), "standard");
ok("Zahl → standard", stilAusWert(3), "standard");
ok("Platzhalter der App → standard", stilAusWert("unbekannt"), "standard");

// ── Welcher Stil wirkt ───────────────────────────────────────────────────
ok("Gerät: Zustand gilt", wirksamerStil("device", "riesen", "hell"), "riesen");
ok("Gerät: URL wird ignoriert", wirksamerStil("device", undefined, "hell"), "standard");
ok("feste URL: ?stil= gilt", wirksamerStil("fixed", "riesen", "hell"), "hell");
ok("feste URL ohne ?stil=", wirksamerStil("fixed", "riesen", null), "standard");
ok("feste URL mit Unsinn", wirksamerStil("fixed", undefined, "quatsch"), "standard");

// ── Panel: Ziel mit Stil ─────────────────────────────────────────────────
ok("Feldwechsel behält den Stil",
  zielMitStil({ kind: "court", court_id: 4 }, "international"),
  { kind: "court", court_id: 4, stil: "international" });
ok("Standard wird weggelassen (bytegleich)",
  zielMitStil({ kind: "court", court_id: 4, stil: "hell" }, "standard"),
  { kind: "court", court_id: 4 });
ok("unbekannter Stil → weggelassen",
  zielMitStil({ kind: "court", court_id: 4 }, "x"), { kind: "court", court_id: 4 });
ok("Tafel kennt keinen Stil",
  zielMitStil({ kind: "court_tafel", court_id: 4 }, "riesen"), { kind: "court_tafel", court_id: 4 });
ok("keine Zuweisung bleibt null", zielMitStil(null, "riesen"), null);

// ── Kurzname ─────────────────────────────────────────────────────────────
ok("Nachname aus family", kurzname("Anna van der Berg", "Anna", "van der Berg"), "van der Berg");
ok("ohne family: letztes Wort", kurzname("Anna Muster", "", ""), "Muster");
ok("Komma-Form: vor dem Komma", kurzname("Muster, Anna", "", ""), "Muster");
ok("einteilig", kurzname("Anna", "", ""), "Anna");
ok("Rand-Leerzeichen", kurzname("  Anna   Muster  ", "", ""), "Muster");
ok("leeres Komma-Vorderteil → voller Name", kurzname(", Anna", "", ""), ", Anna");
ok("leer bleibt leer", kurzname("", "", ""), "");

// ── Plätze am Namen (ADR 0070) ──────────────────────────────────────────
ok("gesetzte Plätze gelten", namenPlaetze({ nameDavor: "logo", nameDarunter: "nation" }),
  { davor: "logo", darunter: "nation" });
ok("ohne Konfiguration: Flagge davor, nichts darunter", namenPlaetze(undefined),
  { davor: "flagge", darunter: "ohne" });
ok("älterer Absender: aus den Schaltern", namenPlaetze({ showNations: false, showClubs: true }),
  { davor: "ohne", darunter: "verein" });
ok("Unsinn → Ableitung", namenPlaetze({ nameDavor: "x\"]", nameDarunter: 3, showNations: true }),
  { davor: "flagge", darunter: "ohne" });

// ── Inline-Kopie in monitor.html ─────────────────────────────────────────
{
  const hier = dirname(fileURLToPath(import.meta.url));
  const modul = readFileSync(join(hier, "..", "src", "io", "monitorStil.mjs"), "utf8");
  const html = readFileSync(join(hier, "..", "src-tauri", "assets", "monitor.html"), "utf8");
  const erwartet = modul.split("\n\n").slice(1).join("\n\n").replace(/^export /gm, "").trim();
  const m = html.match(/\/\* <monitorStil\.mjs> \*\/\n([\s\S]*?)\n\/\* <\/monitorStil\.mjs> \*\//);
  ok("monitor.html trägt die Kopie", !!m, true);
  ok("Kopie gleicht dem Modul", m && m[1].trim() === erwartet, true);
  ok("data-stil kommt nur über wirksamerStil", /dataset\.stil\s*=\s*wirksamerStil\(/.test(html), true);
  for (const s of STILE.filter((x) => x !== "standard")) {
    ok(`CSS für ${s} vorhanden`, html.includes(`[data-stil="${s}"]`), true);
  }
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
