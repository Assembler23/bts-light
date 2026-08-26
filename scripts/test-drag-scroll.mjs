// Testet das Auto-Scroll-Tempo der Umsortier-Ziehgeste (src/io/dragScroll.mjs,
// Spec spielliste-manuelle-reihenfolge) — das echte Modul, dessen Inline-Kopie
// `assets/tl.html` trägt.
//
// Der teuerste Fehler hier ist ein Tempo 0 dort, wo gescrollt werden müsste:
// Dann steht die Liste beim Ziehen still und die Geste ist wieder so hakelig
// wie vor der Änderung. Der zweitteuerste ist ein Tempo ungleich 0 in der
// Mitte — dann wandert die Liste, während man in Ruhe einsortieren will.
import { ziehScrollTempo, scrollSchritt, ueberSchwelle,
  SCROLL_ZONE_PX, MAX_SCROLL_PX_S, ZUG_SCHWELLE_PX }
  from "../src/io/dragScroll.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}
function pruefe(name, bedingung, hinweis = "") {
  if (!bedingung) {
    console.error(`✗ ${name}${hinweis ? `: ${hinweis}` : ""}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// Ein Panel von 100 bis 700 (600 px hoch) — die Zone ist 60 px je Rand.
const OBEN = 100, UNTEN = 700;
const t = (y) => ziehScrollTempo(y, OBEN, UNTEN);

// ── Die ruhige Mitte ──────────────────────────────────────────────────────
ok("Mitte scrollt nicht", t(400), 0);
ok("genau an der oberen Zonengrenze noch nicht", t(OBEN + SCROLL_ZONE_PX), 0);
ok("genau an der unteren Zonengrenze noch nicht", t(UNTEN - SCROLL_ZONE_PX), 0);

// ── Richtungen ────────────────────────────────────────────────────────────
pruefe("oben in der Zone scrollt nach oben", t(OBEN + 10) < 0, `war ${t(OBEN + 10)}`);
pruefe("unten in der Zone scrollt nach unten", t(UNTEN - 10) > 0, `war ${t(UNTEN - 10)}`);

// ── Höchsttempo am und jenseits des Randes ────────────────────────────────
// Der Zeiger DARF das Panel verlassen (Feldtest-Beschwerde „friert ein"):
// Wer nach unten aus dem Panel herausfährt, meint eindeutig „weiter runter".
ok("genau am unteren Rand volles Tempo", t(UNTEN), MAX_SCROLL_PX_S);
ok("weit unterhalb des Panels volles Tempo", t(UNTEN + 500), MAX_SCROLL_PX_S);
ok("genau am oberen Rand volles Tempo", t(OBEN), -MAX_SCROLL_PX_S);
ok("weit oberhalb des Panels volles Tempo", t(OBEN - 500), -MAX_SCROLL_PX_S);

// ── Quadratische Rampe: feine Kontrolle nahe der Zonengrenze ──────────────
// Auf halber Zone ein Viertel des Tempos, nicht die Hälfte — das ist der
// Unterschied zwischen „am Rand noch genau treffen" und „rutscht weg".
ok("halbe Zone = ein Viertel Tempo", t(UNTEN - SCROLL_ZONE_PX / 2), MAX_SCROLL_PX_S / 4);
pruefe("Tempo wächst streng zum Rand hin",
  t(UNTEN - 40) < t(UNTEN - 20) && t(UNTEN - 20) < t(UNTEN - 5));

// ── Flaches Panel: die Zonen dürfen sich nicht aufheben ───────────────────
// Ein zugeklapptes/kurzes Panel ist schmaler als zwei Zonen. Ohne Deckel
// läge die Mitte in BEIDEN Zonen und das Tempo hinge vom Zufall der
// Fallunterscheidung ab.
const flach = (y) => ziehScrollTempo(y, 0, 40);
ok("flaches Panel: Mitte ruht", flach(20), 0);
pruefe("flaches Panel: oben scrollt hoch", flach(2) < 0);
pruefe("flaches Panel: unten scrollt runter", flach(38) > 0);

// ── Unfug schadet nicht ───────────────────────────────────────────────────
ok("Panel ohne Höhe scrollt nicht", ziehScrollTempo(50, 100, 100), 0);
ok("umgekehrte Kanten scrollen nicht", ziehScrollTempo(50, 700, 100), 0);
ok("NaN scrollt nicht", ziehScrollTempo(NaN, OBEN, UNTEN), 0);
ok("undefined scrollt nicht", ziehScrollTempo(undefined, OBEN, UNTEN), 0);

// ── Schritt je Bildtakt ───────────────────────────────────────────────────
ok("900 px/s in 1000 ms sind 900 px", scrollSchritt(900, 1000, 1000), 900);
ok("900 px/s in einem 60-Hz-Takt sind 15 px",
  Math.round(scrollSchritt(900, 1000 / 60) * 1e6) / 1e6, 15);
// Der Deckel ist der Schutz gegen den verschluckten Bildtakt: Ohne ihn
// spränge die Liste nach einem Tab-Wechsel um Tausende Pixel, ohne dass der
// Nutzer den Zeiger bewegt hätte.
ok("verschluckter Takt wird gedeckelt", scrollSchritt(900, 3000), scrollSchritt(900, 50));
ok("negative Zeit scrollt nicht", scrollSchritt(900, -5), 0);
ok("Tempo 0 scrollt nicht", scrollSchritt(0, 16), 0);
ok("NaN-Tempo scrollt nicht", scrollSchritt(NaN, 16), 0);

// ── Tipp oder Zug? ────────────────────────────────────────────────────────
// Der Wächter gegen einen Fehler, der im Feldtest teuer wäre: Seit das
// Scrollen aus der ZEIGERPOSITION kommt, würde ein bloßes Gedrückthalten am
// Griff der obersten Zeile die Liste scrollen und die Reihenfolge ändern —
// gemessen 101 px und drei Plätze nach 300 ms, ohne dass der Finger sich
// bewegt hat. Deshalb passiert bis zur Schwelle GAR NICHTS.
ok("kein Zug ohne jede Bewegung", ueberSchwelle(100, 100, 100, 100), false);
ok("knapp unter der Schwelle ist noch kein Zug",
  ueberSchwelle(100, 100, 100 + ZUG_SCHWELLE_PX - 1, 100), false);
ok("genau auf der Schwelle ist ein Zug",
  ueberSchwelle(100, 100, 100 + ZUG_SCHWELLE_PX, 100), true);
ok("senkrecht zählt genauso",
  ueberSchwelle(100, 100, 100, 100 + ZUG_SCHWELLE_PX), true);
ok("Richtung egal (nach oben/links)",
  ueberSchwelle(100, 100, 100, 100 - ZUG_SCHWELLE_PX), true);
pruefe("schräg unter der Schwelle bleibt Tipp",
  ueberSchwelle(100, 100, 103, 103) === false);
// Im Zweifel KEIN Zug: Unfug darf niemals eine Reihenfolge verschieben.
ok("NaN ist kein Zug", ueberSchwelle(NaN, 100, 200, 200), false);
ok("undefined ist kein Zug", ueberSchwelle(undefined, undefined, 200, 200), false);

if (failures) {
  console.error(`\n${failures} Fehler.`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
