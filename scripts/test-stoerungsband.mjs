// Testet die Zeilen des Störungsbands im TL-Web (src/io/stoerungsband.mjs)
// und schneidet die Inline-Kopie aus tl.html, um sie auf denselben Eingaben
// gegen das Modul laufen zu lassen. Das Band ist die Stelle, an der die
// Turnierleitung einen Ruf vom Feld sieht — eine abweichende Kopie
// verschwiege ihn oder nennte das falsche Feld.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { feldMitHalle, rufText, stoerungsZeilen } from "../src/io/stoerungsband.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const feld = (court, extra = {}) => ({ court, location: "", official_call: false, ...extra });

ok("Feld ohne Halle", feldMitHalle(feld("3")), "3");
ok("Feld mit Halle", feldMitHalle(feld("3", { location: "Halle A" })), "3 (Halle A)");

ok("niemand ruft", rufText([feld("1"), feld("2")]), "");
ok("keine Felder", rufText([]), "");
ok("kein Zustand", rufText(undefined), "");
ok("ein Feld ruft",
  rufText([feld("1"), feld("2", { official_call: true })]),
  "📣 Turnierleitung ans Feld gerufen: Feld 2");
ok("mehrere Felder, mit Halle, in Feld-Reihenfolge",
  rufText([
    feld("1", { official_call: true, location: "Halle A" }),
    feld("2", { location: "Halle A" }),
    feld("7", { official_call: true, location: "Halle B" }),
  ]),
  "📣 Turnierleitung ans Feld gerufen: Feld 1 (Halle A), Feld 7 (Halle B)");
// Eine Verletzung allein ist kein Ruf — die hat ihr eigenes Abzeichen.
ok("Verletzung ist kein Ruf", rufText([feld("1", { injury: true })]), "");

ok("alles ruhig → kein Band",
  stoerungsZeilen({ offline: "", ruf: "", fertig: "", meetingpoint: "" }), []);
ok("nur der Ruf",
  stoerungsZeilen({ offline: "", ruf: "R", fertig: "", meetingpoint: "" }), ["R"]);
ok("Ruf steht vor den anderen Meldungen",
  stoerungsZeilen({ offline: "", ruf: "R", fertig: "F", meetingpoint: "M" }), ["R", "F", "M"]);
ok("ohne Ruf bleiben die anderen",
  stoerungsZeilen({ offline: "", ruf: "", fertig: "F", meetingpoint: "M" }), ["F", "M"]);
ok("ohne Verbindung steht nur die Verbindungsmeldung",
  stoerungsZeilen({ offline: "O", ruf: "R", fertig: "F", meetingpoint: "M" }), ["O"]);

// ── Inline-Kopie in tl.html ──
function funktion(text, name) {
  const start = text.indexOf("function " + name + "(");
  if (start < 0) return null;
  let tiefe = 0;
  for (let i = text.indexOf("{", text.indexOf(")", start)); i < text.length; i++) {
    if (text[i] === "{") tiefe++;
    else if (text[i] === "}" && --tiefe === 0) return text.slice(start, i + 1);
  }
  return null;
}
const hier = dirname(fileURLToPath(import.meta.url));
const tl = readFileSync(join(hier, "..", "src-tauri", "assets", "tl.html"), "utf8");
const namen = ["feldMitHalle", "rufText", "stoerungsZeilen"];
const quellen = namen.map((n) => funktion(tl, n));
ok("tl.html trägt alle drei Kopien", quellen.map((q) => !!q), [true, true, true]);
if (quellen.every(Boolean)) {
  // eslint-disable-next-line no-new-func
  const kopie = new Function(quellen.join("\n") + `\nreturn { ${namen.join(", ")} };`)();
  const brett = [
    [],
    [feld("1")],
    [feld("1", { official_call: true })],
    [feld("1", { official_call: true, location: "Halle A" }), feld("2"),
      feld("Court 9", { official_call: true, location: "Halle B" })],
  ];
  ok("Kopie: rufText wie das Modul",
    brett.map((b) => kopie.rufText(b)), brett.map((b) => rufText(b)));
  const lagen = [];
  for (const offline of ["", "O"]) for (const ruf of ["", "R"])
    for (const fertig of ["", "F"]) for (const meetingpoint of ["", "M"])
      lagen.push({ offline, ruf, fertig, meetingpoint });
  ok("Kopie: stoerungsZeilen wie das Modul",
    lagen.map((l) => kopie.stoerungsZeilen(l)), lagen.map((l) => stoerungsZeilen(l)));
  // Das Band muss die Kopien auch BENUTZEN — sonst wären sie grün und tot.
  const band = funktion(tl, "renderConnection") || "";
  ok("renderConnection nutzt rufText und stoerungsZeilen",
    [band.includes("rufText("), band.includes("stoerungsZeilen(")], [true, true]);
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
