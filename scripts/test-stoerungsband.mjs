// Testet die Zeilen des Störungsbands im TL-Web (src/io/stoerungsband.mjs)
// und schneidet die Inline-Kopie aus tl.html, um sie auf denselben Eingaben
// gegen das Modul laufen zu lassen. Das Band ist die Stelle, an der die
// Turnierleitung einen Ruf vom Feld sieht — eine abweichende Kopie
// verschwiege ihn oder nennte das falsche Feld.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { RUF_EINLEITUNG, feldMitHalle, rufFelder, rufText, stoerungsZeilen } from "../src/io/stoerungsband.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// `court_id` ist bewusst NICHT die Feldnummer: Die Rücknahme muss die
// Kennung des Felds tragen, nicht seinen Namen.
const feld = (court, extra = {}) => ({
  court, court_id: 100 + (parseInt(court, 10) || 0), location: "", official_call: false, ...extra,
});

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

// Der Knopf „zurücknehmen" je Feld braucht Kennung und Beschriftung.
ok("rufFelder: niemand ruft", rufFelder([feld("1")]), []);
ok("rufFelder: kein Zustand", rufFelder(undefined), []);
ok("rufFelder: Kennung und Name je rufendem Feld",
  rufFelder([
    feld("1", { official_call: true, location: "Halle A" }),
    feld("2"),
    feld("7", { official_call: true }),
  ]),
  [{ courtId: 101, name: "Feld 1 (Halle A)" }, { courtId: 107, name: "Feld 7" }]);
// Das Band baut die Ruf-Zeile aus Einleitung + Namen selbst zusammen (mit
// Knöpfen dazwischen) — ohne Knöpfe muss genau `rufText` herauskommen.
{
  const brett = [feld("1", { official_call: true }), feld("7", { official_call: true, location: "B" })];
  ok("Einleitung + Namen ergeben den Ruf-Text",
    RUF_EINLEITUNG + rufFelder(brett).map((f) => f.name).join(", "), rufText(brett));
}

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
const namen = ["feldMitHalle", "rufFelder", "rufText", "stoerungsZeilen"];
const quellen = namen.map((n) => funktion(tl, n));
ok("tl.html trägt alle vier Kopien", quellen.map((q) => !!q), [true, true, true, true]);
const einleitung = tl.match(/^const RUF_EINLEITUNG = .*;$/m);
ok("tl.html trägt die Einleitung der Ruf-Zeile", !!einleitung, true);
if (quellen.every(Boolean) && einleitung) {
  // eslint-disable-next-line no-new-func
  const kopie = new Function(
    einleitung[0] + "\n" + quellen.join("\n") + `\nreturn { RUF_EINLEITUNG, ${namen.join(", ")} };`,
  )();
  ok("Kopie: Einleitung wie das Modul", kopie.RUF_EINLEITUNG, RUF_EINLEITUNG);
  const brett = [
    [],
    [feld("1")],
    [feld("1", { official_call: true })],
    [feld("1", { official_call: true, location: "Halle A" }), feld("2"),
      feld("Court 9", { official_call: true, location: "Halle B" })],
  ];
  ok("Kopie: rufText wie das Modul",
    brett.map((b) => kopie.rufText(b)), brett.map((b) => rufText(b)));
  ok("Kopie: rufFelder wie das Modul",
    brett.map((b) => kopie.rufFelder(b)), brett.map((b) => rufFelder(b)));
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
  // Der Knopf „zurücknehmen": nur wenn der Turnier-PC die Aktion kennt (ein
  // älterer Stand verwürfe sie still), und er sendet die Kennung des Felds.
  ok("das Band zeigt den Knopf nur mit der Fähigkeit des Turnier-PCs",
    [band.includes("can_clear_official_call"), band.includes("rufFelder(")], [true, true]);
  const zeichnen = funktion(tl, "zeichneBand") || "";
  ok("zeichneBand setzt je Feld einen Knopf, der die Rücknahme auslöst",
    [zeichnen.includes("RUF_EINLEITUNG"), zeichnen.includes("nimmRufZurueck(feld.courtId)")], [true, true]);
  const ruecknahme = funktion(tl, "nimmRufZurueck") || "";
  ok("nimmRufZurueck fragt nach und sendet clear_official_call",
    [ruecknahme.includes("confirm("), ruecknahme.includes('action: "clear_official_call", courtId')],
    [true, true]);
}

// ── Tablet-Seite: die Nachricht `official_call_cleared` ──────────────────
// Der ECHTE Code aus tablet.html läuft gegen Attrappen. Zwei Dinge dürfen
// nie kippen: Das Megafon geht aus UND der Zustand wird gesichert (sonst
// meldete das nächste Neuladen den erledigten Ruf erneut), und die Antwort
// geht IMMER hinaus — der Turnier-PC hält neue Rufe des Felds zurück, bis
// er „kein Ruf" gehört hat.
{
  const tablet = readFileSync(join(hier, "..", "src-tauri", "assets", "tablet.html"), "utf8");
  const quelle = funktion(tablet, "beiRufZurueckgenommen");
  ok("tablet.html: beiRufZurueckgenommen vorhanden", !!quelle, true);
  ok("tablet.html: die Nachricht ist verdrahtet",
    /msg\.type === 'official_call_cleared'[^\n]*beiRufZurueckgenommen\(\)/.test(tablet), true);
  // Beim Verbinden meldet das Tablet seine Meldungen IMMER — auch „keine".
  const oeffnen = tablet.slice(tablet.indexOf("ws.onopen = () => {"), tablet.indexOf("ws.onmessage"));
  ok("tablet.html: beim Verbinden wird unbedingt gemeldet", /^\s*sendAlert\(\);$/m.test(oeffnen), true);
  if (quelle) {
    const lauf = (alertOfficial) => {
      const ruf = [];
      const STATE = { alertOfficial, alertInjury: true };
      const merk = (name) => (...a) => { ruf.push(a.length ? `${name}:${a[0]}` : name); };
      const els = { megaphoneModal: { classList: { remove: merk("dialog-zu") } } };
      new Function("STATE", "els", "persistState", "zeigeToast", "render", "sendAlert",
        quelle + "\nbeiRufZurueckgenommen();")(
        STATE, els, merk("persistState"), merk("zeigeToast"), merk("render"),
        () => ruf.push(`sendAlert:${STATE.alertOfficial}`));
      return { ruf, STATE };
    };
    const mit = lauf(true);
    ok("Tablet: Megafon aus, Dialog zu, gesichert, Hinweis, dann bestätigt",
      mit.ruf, ["dialog-zu:visible", "persistState", "zeigeToast:ruf-toast", "render", "sendAlert:false"]);
    // Die Verletzungsmeldung gehört dem Tablet — die Rücknahme rührt sie nicht an.
    ok("Tablet: die Verletzungsmeldung bleibt stehen", mit.STATE.alertInjury, true);
    ok("Tablet: ohne stehenden Ruf nur die Bestätigung", lauf(false).ruf, ["sendAlert:false"]);
  }

  // ── Meldungen nachholen, sobald das Gerät das Feld hält ────────────────
  // Turnier-PC und Relay verwerfen `alert` von einem Gerät, das das Feld
  // nicht hält. Ohne Nachholen bliebe nach einer Übernahme eine Rücknahme
  // der Turnierleitung unbestätigt offen — und der erste echte Ruf des
  // neuen Geräts würde zurückgehalten.
  const nachholen = funktion(tablet, "meldungenNachholen");
  ok("tablet.html: meldungenNachholen vorhanden", !!nachholen, true);
  if (nachholen) {
    const lauf = (offen) => {
      let gesendet = 0;
      const danach = new Function("sendAlert",
        `let MELDUNGEN_OFFEN = ${offen};\n${nachholen}\n`
        + "meldungenNachholen(); meldungenNachholen(); return MELDUNGEN_OFFEN;")(
        () => { gesendet++; });
      return { gesendet, danach };
    };
    ok("Tablet: vorgemerkt → genau EINE Meldung, Merker gelöscht", lauf(true), { gesendet: 1, danach: false });
    ok("Tablet: nichts vorgemerkt → keine Meldung", lauf(false), { gesendet: 0, danach: false });
  }
  const belegt = tablet.slice(tablet.indexOf("msg.type === 'court_occupied'"),
    tablet.indexOf("msg.type === 'session_superseded'"));
  ok("tablet.html: „Feld belegt“ merkt die Meldungen vor", /MELDUNGEN_OFFEN = true;/.test(belegt), true);
  ok("tablet.html: nach der Zuweisung wird nachgeholt",
    /msg\.type === 'match_assigned'\) \{\s*beiMatchAssigned\(msg\);\s*meldungenNachholen\(\);/.test(tablet), true);
  ok("tablet.html: auch ohne Spiel wird nachgeholt",
    /msg\.type === 'match_cleared'[^\n]*clearMatch\(\); meldungenNachholen\(\);/.test(tablet), true);

  const uebernehmen = funktion(tablet, "feldUebernehmen");
  ok("tablet.html: feldUebernehmen vorhanden und am Knopf",
    !!uebernehmen && tablet.includes("els.takeoverBtn.addEventListener('click', feldUebernehmen);"), true);
  if (uebernehmen) {
    const ruf = [];
    const merk = (name) => () => { ruf.push(name); };
    const STATE = { occupied: true, locked: true };
    const ws = { readyState: 1, send: (t) => ruf.push(JSON.parse(t).type) };
    const danach = new Function("STATE", "ws", "WebSocket", "DEVICE_ID", "clearReidentify", "tlog", "render", "sendAlert",
      `let MELDUNGEN_OFFEN = false;\n${uebernehmen}\nfeldUebernehmen(); return MELDUNGEN_OFFEN;`)(
      STATE, ws, { OPEN: 1 }, "geraet-1", merk("clearReidentify"), () => {}, merk("render"), merk("sendAlert"));
    // NICHT sofort melden: Der übernommene Stand kommt erst noch. Ein
    // „kein Ruf" an dieser Stelle löschte den Merker am Turnier-PC, und der
    // übernommene (schon zurückgenommene) Ruf stünde danach wieder im Band.
    ok("Tablet: Übernahme sendet take_over, meldet aber noch nicht",
      ruf, ["clearReidentify", "take_over", "render"]);
    ok("Tablet: Übernahme merkt die Meldungen vor", danach, true);
    ok("Tablet: Übernahme hebt Sperre und „belegt“ auf", STATE, { occupied: false, locked: false });
  }
}

// ── TL-Web: Tooltip des Menüeintrags ─────────────────────────────────────
// Ein gerades Anführungszeichen im Text beendete das `title`-Attribut — der
// Tooltip hörte nach „TL gerufen" auf.
{
  const tl = readFileSync(join(hier, "..", "src-tauri", "assets", "tl.html"), "utf8");
  ok("tl.html: der Tooltip von „Ruf zurücknehmen“ reicht bis zum Ende",
    /data-clearcall="\$\{c\.court_id\}"\s*title="[^"]*Megafon aus">/.test(tl), true);
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
