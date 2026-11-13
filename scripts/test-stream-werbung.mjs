// Testet die Logik der Stream-Werbung (src/io/streamWerbung.mjs, Spec
// stream-werbung, ADR 0092), die Inline-Kopie in stream-werbung.html und den
// Adressbau des Baukastens. Hier entscheidet sich vor allem, wann die Ebene
// stehen bleibt (jede Antwort außer „200 mit Liste") — ein Relay-Neustart
// mitten in der Übertragung darf die Sponsoren nicht leeren.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { streamWerbungLink } from "../src/io/streamLink.mjs";
import {
  MODI, HINTERGRUENDE, TEMPO_PX_S, VORGABE, ABSTAND_PX, INNENRAND_PX, ABRUF_MS,
  KACHEL_ERSATZ, optionenAusQuery, entscheidung, stilFarbe, bildUrl, streamBilder,
  listenSchluessel, reiheMassstab, bandWiederholungen, bandDauerS, rotationIntervallMs,
} from "../src/io/streamWerbung.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Optionen aus der Adresse ─────────────────────────────────────────────
ok("Modi", MODI, ["rotation", "reihe", "band"]);
ok("Hintergründe", HINTERGRUENDE, ["transparent", "kachel"]);
ok("Tempo in px/s", TEMPO_PX_S, { langsam: 40, mittel: 80, schnell: 140 });
ok("ohne Parameter die Vorgaben", optionenAusQuery(null, null, null),
  { modus: "rotation", hintergrund: "transparent", tempo: "mittel" });
ok("gültige Werte", optionenAusQuery("band", "kachel", "schnell"),
  { modus: "band", hintergrund: "kachel", tempo: "schnell" });
ok("Unsinn fällt auf die Vorgabe", optionenAusQuery("BAND", "<x>", "toString"), VORGABE);
ok("Prototyp-Namen sind kein Tempo", optionenAusQuery("band", null, "__proto__").tempo, "mittel");

// ── Entscheidung nach dem Abruf ──────────────────────────────────────────
ok("200 mit Liste → anwenden", entscheidung(200, { streamAds: ["0"] }), "anwenden");
ok("200 mit leerer Liste → anwenden (Ebene leert sich)", entscheidung(200, { streamAds: [] }), "anwenden");
ok("503 keine Daten → behalten", entscheidung(503, { grund: "keine_daten" }), "behalten");
ok("304 → behalten", entscheidung(304, null), "behalten");
ok("500 → behalten", entscheidung(500, null), "behalten");
ok("Netzfehler (0) → behalten", entscheidung(0, null), "behalten");
ok("alter Relay ohne streamAds → behalten", entscheidung(200, { ads: ["0"] }), "behalten");
ok("kaputtes JSON → behalten", entscheidung(200, null), "behalten");

// ── Kachelfarbe ──────────────────────────────────────────────────────────
ok("Ersatz ist Schwarz", KACHEL_ERSATZ, "#000000");
ok("gesetzte Farbe", stilFarbe(["a", "b"], [{}, { bg: "#ffcc00" }], "b"), "#ffcc00");
ok("leer → Schwarz", stilFarbe(["a"], [{ bg: "" }], "a"), "#000000");
ok("#000000 bleibt Schwarz", stilFarbe(["a"], [{ bg: "#000000" }], "a"), "#000000");
ok("ungültig → Schwarz", stilFarbe(["a"], [{ bg: "red;background:url(x)" }], "a"), "#000000");
ok("ohne Stil-Liste (LAN ohne Stile) → Schwarz", stilFarbe(["a"], [], "a"), "#000000");
ok("unbekanntes Bild → Schwarz", stilFarbe(["a"], [{ bg: "#ffffff" }], "z"), "#000000");

// ── Bild-Adressen und Liste ──────────────────────────────────────────────
ok("LAN ohne Version", bildUrl("/", "ad-1.png", ""), "/ads/ad-1.png");
ok("Cloud mit Version", bildUrl("/bts-relay/ns/", "2", "img-5-ab"), "/bts-relay/ns/ads/2?v=img-5-ab");
ok("Kennung wird kodiert", bildUrl("/", "a b?.png", ""), "/ads/a%20b%3F.png");
const zustand = {
  ads: ["0", "1", "2"], adStyles: [{ bg: "#ffffff" }, {}, { bg: "" }],
  streamAds: ["0", "2"], streamVer: ["v0", "v2"], intervalS: 8,
};
ok("Stream-Bilder in Reihenfolge mit Farbe und Version", streamBilder(zustand, "/b/"), [
  { id: "0", url: "/b/ads/0?v=v0", bg: "#ffffff" },
  { id: "2", url: "/b/ads/2?v=v2", bg: "#000000" },
]);
ok("ohne streamVer (LAN)", streamBilder({ ads: ["x.png"], streamAds: ["x.png"] }, "/")[0].url, "/ads/x.png");
ok("leerer Zustand", streamBilder({}, "/"), []);
const a = streamBilder(zustand, "/");
ok("gleiche Liste, gleicher Schlüssel", listenSchluessel(a, true), listenSchluessel(streamBilder(zustand, "/"), true));
ok("neue Version → neuer Schlüssel", listenSchluessel(a, false) === listenSchluessel(
  streamBilder({ ...zustand, streamVer: ["v0", "neu"] }, "/"), false), false);
const andereFarbe = streamBilder({ ...zustand, adStyles: [{ bg: "#111111" }, {}, {}] }, "/");
ok("neue Farbe mit Kachel → neuer Schlüssel", listenSchluessel(a, true) === listenSchluessel(andereFarbe, true), false);
ok("neue Farbe transparent → gleicher Schlüssel", listenSchluessel(a, false) === listenSchluessel(andereFarbe, false), true);

// ── Maße ─────────────────────────────────────────────────────────────────
ok("Reihe passt → 1", reiheMassstab(800, 1920), 1);
ok("Reihe zu breit → gemeinsam kleiner", reiheMassstab(3840, 1920), 0.5);
ok("Reihe ohne Breite → 1", reiheMassstab(0, 1920), 1);
ok("Band: ein schmales Bild wird wiederholt", bandWiederholungen(300, 1920), 7);
ok("Band: breite Folge einmal", bandWiederholungen(2500, 1920), 1);
ok("Band: ohne Breite einmal", bandWiederholungen(0, 1920), 1);
ok("Band-Dauer", bandDauerS(300, 7, 80), 26.25);
ok("Band-Dauer ohne Tempo", bandDauerS(300, 7, 0), 0);
ok("Rotation im Werbe-Intervall", rotationIntervallMs(8), 8000);
ok("Rotation mindestens 1 s", rotationIntervallMs(0.2), 1000);
ok("Rotation ohne Angabe 10 s", rotationIntervallMs(undefined), 10000);
ok("Abruf alle 20 s", ABRUF_MS, 20000);

// ── Inline-Kopie und Seitenregeln ────────────────────────────────────────
{
  const hier = dirname(fileURLToPath(import.meta.url));
  const modul = readFileSync(join(hier, "..", "src", "io", "streamWerbung.mjs"), "utf8");
  const html = readFileSync(join(hier, "..", "src-tauri", "assets", "stream-werbung.html"), "utf8");
  const erwartet = modul.split("\n\n").slice(1).join("\n\n").replace(/^export /gm, "").trim();
  const m = html.match(/\/\* <streamWerbung\.mjs> \*\/\n([\s\S]*?)\n\/\* <\/streamWerbung\.mjs> \*\//);
  ok("stream-werbung.html trägt die Kopie", !!m, true);
  ok("Kopie gleicht dem Modul", m && m[1].trim() === erwartet, true);
  ok("Seite nutzt den Basis-Platzhalter", html.includes('var BASE = "__BASE__";'), true);
  ok("Abruf mit zweck=stream", html.includes('info/ad/state?zweck=stream'), true);
  ok("Seite fragt nur per GET (kein method:)", /method\s*:/.test(html), false);
  ok("kein innerHTML", /innerHTML/.test(html), false);
  ok("kein textContent (nie Text im Bild)", /textContent|innerText/.test(html), false);
  ok("Hintergrund transparent", /html, body \{[^}]*background: transparent/.test(html), true);
  ok("keine Geräte-ID", /device=/.test(html), false);
  ok("Abstand im CSS wie im Modul", html.includes(`gap: ${ABSTAND_PX}px`) && html.includes(`margin-right: ${ABSTAND_PX}px`), true);
  ok("Innenrand im CSS wie im Modul", html.includes(`padding: ${INNENRAND_PX}px`), true);
}

// ── Baukasten: gebaute Adresse liest die Seite genau so ──────────────────
{
  const lan = "http://bts-light.local:8088";
  ok("Vorgaben ohne Parameter", streamWerbungLink(lan, {}), lan + "/stream/werbung");
  ok("Reihe mit Kachel", streamWerbungLink(lan, { modus: "reihe", hintergrund: "kachel" }),
    lan + "/stream/werbung?modus=reihe&hintergrund=kachel");
  ok("Tempo nur beim Band", streamWerbungLink(lan, { modus: "reihe", tempo: "schnell" }),
    lan + "/stream/werbung?modus=reihe");
  ok("Band langsam", streamWerbungLink(lan, { modus: "band", tempo: "langsam" }),
    lan + "/stream/werbung?modus=band&tempo=langsam");
  ok("Band mittel ohne tempo", streamWerbungLink(lan, { modus: "band", tempo: "mittel" }),
    lan + "/stream/werbung?modus=band");
  ok("ohne Basis leer", streamWerbungLink("", {}), "");
  ok("Schrägstrich am Ende", streamWerbungLink("https://badhub.de/bts-relay/abc/", {}),
    "https://badhub.de/bts-relay/abc/stream/werbung");
  for (const opt of [
    { modus: "band", hintergrund: "kachel", tempo: "schnell" },
    { modus: "reihe", hintergrund: "transparent", tempo: "mittel" },
    { modus: "rotation", hintergrund: "kachel", tempo: "mittel" },
  ]) {
    const u = new URL(streamWerbungLink(lan, opt));
    ok(`Rundweg ${opt.modus}/${opt.hintergrund}`,
      optionenAusQuery(u.searchParams.get("modus"), u.searchParams.get("hintergrund"), u.searchParams.get("tempo")),
      opt);
  }
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
