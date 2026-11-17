// Testet die Ziele der badhub-Links der TL-Sicht (src/io/badhubLinks.mjs) und
// lässt die Inline-Kopie aus tl.html auf denselben Eingaben gegen das Modul
// laufen. Eine abweichende Kopie schickte die Turnierleitung eines
// Testturniers auf das Produktivsystem — oder auf ein fremdes Turnier.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as bl from "../src/io/badhubLinks.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const G = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA";
const TEST = { base: "https://test.badhub.de", guid: G, liveticker: `https://test.badhub.de/live?t=bvbb&g=${G}` };

ok("Basis vom Host (Testsystem)", bl.badhubBasis(TEST, "", "http://192.168.1.2:8088"), "https://test.badhub.de");
ok("älterer Host im Hallennetz: Produktiv", bl.badhubBasis(undefined, "", "http://192.168.1.2:8088"), "https://badhub.de");
ok("älterer Host in der Cloud: Host der Seite", bl.badhubBasis({}, "/bts-relay", "https://test.badhub.de"), "https://test.badhub.de");
ok("Cloud: Basis immer vom eigenen Host, nie vom Turnier-PC", bl.badhubBasis({ base: "https://boese.example" }, "/bts-relay", "https://badhub.de"), "https://badhub.de");
ok("Basis mit Pfad/Schema-Unsinn verworfen", [
  bl.badhubBasis({ base: "javascript:alert(1)" }, "", "x"),
  bl.badhubBasis({ base: "https://badhub.de/" }, "", "x"),
  bl.badhubBasis({ base: "https://a.de\"><b" }, "", "x"),
], ["https://badhub.de", "https://badhub.de", "https://badhub.de"]);
ok("eigene Instanz mit Port", bl.badhubBasis({ base: "http://10.0.0.5:8000" }, "", "x"), "http://10.0.0.5:8000");

const B = "https://test.badhub.de";
ok("Turnierseite", bl.turnierSprung(TEST, B, {}), `${B}/checkin/${G}/turnier`);
ok("ohne GUID kein Sprung", bl.turnierSprung({ base: B }, B, {}), "");
ok("kaputte GUID kein Sprung", bl.turnierSprung({ guid: "../x" }, B, {}), "");
ok("Spieler (im Turnier)", bl.spielerLink(TEST, B, " 05-123456 "), `${B}/checkin/${G}/turnier?player=05-123456`);
ok("Spieler ohne GUID: globale Weiterleitung", bl.spielerLink({}, "https://badhub.de", "05-1"), "https://badhub.de/spieler/05-1/live");
ok("Spieler ohne Nummer: kein Link", bl.spielerLink(TEST, B, ""), "");
ok("Nummer wird kodiert", bl.spielerLink({}, B, "a/b"), `${B}/spieler/a%2Fb/live`);
ok("Konkurrenz über Spiel-ID + Name", bl.konkurrenzLink(TEST, B, 935, "MX-B"), `${B}/checkin/${G}/turnier?spiel=935&konk=MX-B`);
ok("Konkurrenz-Name wird kodiert", bl.konkurrenzLink(TEST, B, 7, "HE U19 & mehr"), `${B}/checkin/${G}/turnier?spiel=7&konk=HE+U19+%26+mehr`);
ok("Konkurrenz ohne Name", bl.konkurrenzLink(TEST, B, 7, ""), `${B}/checkin/${G}/turnier?spiel=7`);
ok("Konkurrenz ohne GUID / ohne ID: kein Link",
  [bl.konkurrenzLink({}, B, 7, "HE"), bl.konkurrenzLink(TEST, B, 0, "HE"), bl.konkurrenzLink(TEST, B, "x", "HE")], ["", "", ""]);
ok("Liveticker", bl.livetickerLink(TEST, B), TEST.liveticker);
ok("Liveticker nur http(s)", [bl.livetickerLink({ liveticker: "javascript:alert(1)" }, B), bl.livetickerLink({}, B), bl.livetickerLink(null, B)], ["", "", ""]);
ok("Liveticker nur auf derselben Basis", [bl.livetickerLink(TEST, "https://badhub.de"), bl.livetickerLink({ liveticker: "https://test.badhub.de.boese.example/live" }, B)], ["", ""]);

// ── Inline-Kopie in tl.html ──
function funktion(text, name) {
  const start = text.indexOf("function " + name + "(");
  if (start < 0) return null;
  let tiefe = 0;
  for (let i = text.indexOf("{", start); i < text.length; i++) {
    if (text[i] === "{") tiefe++;
    else if (text[i] === "}" && --tiefe === 0) return text.slice(start, i + 1);
  }
  return null;
}
const hier = dirname(fileURLToPath(import.meta.url));
const tl = readFileSync(join(hier, "..", "src-tauri", "assets", "tl.html"), "utf8");
const NAMEN = ["badhubBasis", "turnierSprung", "spielerLink", "konkurrenzLink", "livetickerLink"];
const quellen = NAMEN.map((n) => funktion(tl, n));
ok("tl.html trägt alle Kopien", quellen.map(Boolean), NAMEN.map(() => true));
if (quellen.every(Boolean)) {
  // eslint-disable-next-line no-new-func
  const k = new Function(quellen.join("\n") + `\nreturn { ${NAMEN.join(", ")} };`)();
  const LINKS = [undefined, null, {}, TEST, { base: B }, { guid: G }, { base: "javascript:x", guid: "kaputt", liveticker: "ftp://x" },
    { base: "http://10.0.0.5:8000", guid: G.toLowerCase(), liveticker: " https://badhub.de/live?t=x " }];
  const WERTE = ["", " 05-1 ", "a b&c", 0, 935, "12", -3, null, undefined];
  for (const n of NAMEN) {
    const faelle = [];
    for (const l of LINKS) for (const w of WERTE) faelle.push([l, w]);
    const rufe = (f) => faelle.map(([l, w]) => {
      switch (n) {
        case "badhubBasis": return [f(l, "", "o"), f(l, "/bts-relay", "https://o.de")];
        case "turnierSprung": return f(l, B, { spiel: w, konk: w, player: w });
        case "spielerLink": return f(l, B, w);
        case "konkurrenzLink": return f(l, B, w, w);
        default: return [f(l, B), f(l, "https://badhub.de")];
      }
    });
    ok(`Kopie ${n} verhält sich wie das Modul`, rufe(k[n]), rufe(bl[n]));
  }
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
