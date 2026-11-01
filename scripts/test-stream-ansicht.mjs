// Testet die Logik der Stream-Einblendung (src/io/streamAnsicht.mjs, Spec
// monitor-stile-und-stream E2, ADR 0069) und die Inline-Kopie in stream.html.
// Hier entscheidet sich, welche Felder eine Adresse anfordern kann, dass die
// Adresse nichts EINschalten kann und wann die Einblendung verschwindet.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { streamLink } from "../src/io/streamLink.mjs";
import {
  MAX_FELDER, DATEN_FRIST_MS, courtsAusQuery, optionenAusQuery,
  zaehltAlsDaten, datenFrisch, feldSichtbar,
} from "../src/io/streamAnsicht.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Felder aus der Adresse ───────────────────────────────────────────────
ok("höchstens drei", MAX_FELDER, 3);
ok("ein Feld", courtsAusQuery("7"), [7]);
ok("drei Felder in Reihenfolge", courtsAusQuery("3,1,2"), [3, 1, 2]);
ok("Doppelte fallen weg, Kappung 3", courtsAusQuery("1,2,2,3,4"), [1, 2, 3]);
ok("Leerzeichen um die IDs", courtsAusQuery(" 4 , 5 "), [4, 5]);
ok("Unsinn fällt weg", courtsAusQuery("abc"), []);
ok("negative/Null/Brüche/Exponent fallen weg", courtsAusQuery("-1,0,1.5,1e3,07"), []);
ok("gemischt", courtsAusQuery("x,9,../,10"), [9, 10]);
ok("fehlt", courtsAusQuery(null), []);
ok("leer", courtsAusQuery(""), []);

// ── Optionen: nur abschalten ─────────────────────────────────────────────
ok("ohne Parameter alles an", optionenAusQuery(null, null, null), { nation: true, verein: true, logo: true });
ok("=0 schaltet ab", optionenAusQuery("0", "0", "0"), { nation: false, verein: false, logo: false });
ok("=1 ändert nichts (Server entscheidet)", optionenAusQuery("1", "1", "1"), { nation: true, verein: true, logo: true });
ok("nur logo aus", optionenAusQuery(null, null, "0"), { nation: true, verein: true, logo: false });

// ── Lebenszeichen und Frist ──────────────────────────────────────────────
ok("Frist 60 s", DATEN_FRIST_MS, 60000);
ok("Stand mit Feldname zählt", zaehltAlsDaten({ courtLabel: "3" }), true);
ok("Relay ohne Host (leerer Name) zählt nicht", zaehltAlsDaten({ courtLabel: "" }), false);
ok("nichts zählt nicht", zaehltAlsDaten(null), false);
ok("frisch", datenFrisch(1000, 1000 + 59999), true);
ok("nach 60 s weg", datenFrisch(1000, 1000 + 60000), false);
ok("ohne je Daten weg", datenFrisch(0, 5), false);
ok("Feld mit Spiel sichtbar", feldSichtbar({ courtLabel: "1", match: {} }), true);
ok("Feld ohne Spiel unsichtbar", feldSichtbar({ courtLabel: "1" }), false);
ok("Spiel ohne Host unsichtbar", feldSichtbar({ courtLabel: "", match: {} }), false);

// ── Inline-Kopie in stream.html ──────────────────────────────────────────
{
  const hier = dirname(fileURLToPath(import.meta.url));
  const modul = readFileSync(join(hier, "..", "src", "io", "streamAnsicht.mjs"), "utf8");
  const html = readFileSync(join(hier, "..", "src-tauri", "assets", "stream.html"), "utf8");
  const erwartet = modul.split("\n\n").slice(1).join("\n\n").replace(/^export /gm, "").trim();
  const m = html.match(/\/\* <streamAnsicht\.mjs> \*\/\n([\s\S]*?)\n\/\* <\/streamAnsicht\.mjs> \*\//);
  ok("stream.html trägt die Kopie", !!m, true);
  ok("Kopie gleicht dem Modul", m && m[1].trim() === erwartet, true);
  ok("Seite nutzt den Basis-Platzhalter", html.includes('var BASE = "__BASE__";'), true);
  ok("Seite fragt nur per GET (kein method:)", /method\s*:/.test(html), false);
  ok("kein innerHTML", /innerHTML/.test(html), false);
  ok("Hintergrund transparent", /html, body \{[^}]*background: transparent/.test(html), true);
  ok("keine Geräte-ID", /device=/.test(html), false);
}

// ── Baukasten im Panel: gebaute Adresse liest die Seite genau so ─────────
{
  const lan = "http://bts-light.local:8088";
  ok("ein Feld", streamLink(lan, [3], {}), lan + "/stream?courts=3");
  ok("Optionen nur als =0", streamLink(lan, [1, 2], { nation: true, verein: false, logo: false }),
    lan + "/stream?courts=1,2&verein=0&logo=0");
  ok("höchstens drei, ohne Doppelte", streamLink(lan, [1, 1, 2, 3, 4], {}), lan + "/stream?courts=1,2,3");
  ok("ohne Feld leer", streamLink(lan, [], {}), "");
  ok("ohne Basis leer", streamLink("", [1], {}), "");
  ok("Schrägstrich am Ende", streamLink("https://badhub.de/bts-relay/abc/", [5], {}),
    "https://badhub.de/bts-relay/abc/stream?courts=5");
  const u = new URL(streamLink(lan, [7, 9], { nation: false }));
  ok("Rundweg Felder", courtsAusQuery(u.searchParams.get("courts")), [7, 9]);
  ok("Rundweg Optionen",
    optionenAusQuery(u.searchParams.get("nation"), u.searchParams.get("verein"), u.searchParams.get("logo")),
    { nation: false, verein: true, logo: true });
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
