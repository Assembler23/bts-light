// Testet, wie eine geladene Tablet-Seite merkt, dass sie veraltet ist — und
// wie sie auf den Fernbefehl der Turnierleitung reagiert.
// Spec: docs/features/tablet-version-abgleich.md
//
// Hintergrund: Ein Turnier-Tablet läuft tagelang mit derselben geladenen
// Seite; ein Update erreicht es nur über ein Neuladen, und das passiert von
// selbst nie (Feldtest 21./22.08.2026).
//
// Die Logik lebt NUR inline in assets/tablet.html und assets/tl.html — die
// Seiten sind selbstenthaltend und haben keinen Build-Schritt. Deshalb wird
// sie hier aus dem HTML herausgeschnitten und über ihre Seiteneffekte
// geprüft, statt ein Modul zu importieren. Verschiebt sich ein Anker, fällt
// der Test aus („Abschnitt nicht gefunden") statt still durchzuwinken.
//
// Drei Fälle tragen das Ganze und sind ohne Test nicht zu halten:
//   * Der stille Abgleich darf mitten im Spiel NICHT neu laden (A3/A4).
//   * Der Fernbefehl MUSS es auch dann tun (B1).
//   * Der Knopf darf an einem älteren Turnier-PC gar nicht erscheinen (B6) —
//     sonst hält die Turnierleitung die Tablets für aufgefrischt, obwohl der
//     Host die Aktion still verwirft.
import fs from "node:fs";

const tablet = fs.readFileSync("src-tauri/assets/tablet.html", "utf8");
const tl = fs.readFileSync("src-tauri/assets/tl.html", "utf8");

let fehler = 0;
const p = (name, ist, soll) => {
  if (JSON.stringify(ist) !== JSON.stringify(soll)) {
    console.error(`✗ ${name}: erwartet ${JSON.stringify(soll)}, war ${JSON.stringify(ist)}`);
    fehler++;
  } else {
    console.log(`✓ ${name}`);
  }
};

/** Schneidet einen Abschnitt aus einer Seite; bricht ab, wenn er fehlt. */
function abschnitt(quelle, von, bis, was) {
  const a = quelle.indexOf(von);
  const b = quelle.indexOf(bis, a + 1);
  if (a < 0 || b <= a) {
    console.error(`✗ ${was}: Abschnitt nicht gefunden — Anker verschoben?`);
    process.exit(1);
  }
  return quelle.slice(a, b);
}

// ══ Teil 1: stiller Abgleich (v0.9.266) ═════════════════════════════════

p("Platzhalter für die Seiten-Marke vorhanden", tablet.includes("'__SEITEN_MARKE__'"), true);

{
  const src = abschnitt(
    tablet,
    "    if (msg.type === 'pong') {",
    "    if (msg.type === 'match_assigned') {",
    "Pong-Zweig",
  );

  /** Fährt den Pong-Zweig einmal und meldet, was er getan hat. */
  function lauf(marke, eigene, state, schonGemerkt = "") {
    const wirkung = { geladen: [], gerendert: 0 };
    const f = new Function(
      "msg", "SEITEN_MARKE", "neueFassungMarke", "STATE", "tlog", "neuLaden", "render",
      src,
    );
    f({ type: "pong", marke }, eigene, schonGemerkt, state,
      () => {}, (m) => wirkung.geladen.push(m), () => { wirkung.gerendert++; });
    return wirkung;
  }

  const frei = { match: null, pendingResult: null };
  const spielt = { match: { id: 1 }, pendingResult: null };

  let r = lauf("aaa", "aaa", frei);
  p("A1 gleiche Marke: kein Reload", r.geladen, []);
  p("A1 gleiche Marke: kein Neuzeichnen", r.gerendert, 0);

  r = lauf("bbb", "aaa", frei);
  p("A2 neue Marke ohne Spiel: lädt sofort", r.geladen, ["bbb"]);

  r = lauf("bbb", "aaa", spielt);
  p("A3 neue Marke mit Spiel: lädt NICHT", r.geladen, []);
  p("A3 neue Marke mit Spiel: zeigt den Hinweis", r.gerendert, 1);

  r = lauf("bbb", "aaa", { match: null, pendingResult: { matchId: 1 } });
  p("A4 offenes Ergebnis: lädt NICHT", r.geladen, []);

  r = lauf("", "aaa", frei);
  p("A5 alter Server ohne Marke: nichts", r.geladen, []);

  r = lauf("bbb", "aaa", frei, "bbb");
  p("A6 schon bekannt: kein zweiter Reload", r.geladen, []);
}

{
  // A7: Neu geladen wird mit der Marke IN DER ADRESSE. Ein schlichtes
  // location.reload() darf der Browser aus dem Zwischenspeicher bedienen —
  // dann käme wieder die alte Seite und der Abgleich liefe endlos.
  const src = abschnitt(tablet, "function neuLaden(marke) {", "// Lebt in localStorage", "neuLaden");
  let ziel = "";
  const neuLaden = new Function("tlog", "uploadTabletLog", "location", src + "\nreturn neuLaden;")(
    () => {}, () => {},
    { href: "https://badhub.de/bts-relay/ns/tablet/7", replace: (u) => { ziel = u; } },
  );

  neuLaden("abc123");
  p("A7 Reload-URL trägt die Marke", /[?&]v=abc123/.test(ziel), true);
  p("A7 Reload behält den Pfad", /\/tablet\/7/.test(ziel), true);

  // Ohne Marke muss ein Zeitstempel einspringen, sonst wäre die Adresse bei
  // jedem Versuch dieselbe — und damit wieder aus dem Cache bedienbar.
  ziel = "";
  neuLaden("");
  p("A7 ohne Marke: trotzdem eine frische Adresse", /[?&]v=\d{10,}/.test(ziel), true);
}

// ══ Teil 2: Fernbefehl (v0.9.268) ═══════════════════════════════════════

{
  const roh = abschnitt(
    tablet,
    "    else if (msg.type === 'reload') {",
    "    else if (msg.type === 'match_cleared') {",
    "Reload-Zweig",
  );
  const src = roh.replace(/^\s*else /, "");

  function lauf(msg, state, speicherGeht = true) {
    const w = { geladen: [], gerendert: 0 };
    const f = new Function(
      "msg", "STATE", "SPEICHER_GEHT", "neueFassungMarke",
      "tlog", "neuLaden", "render", src + "\nreturn 'durchgefallen';");
    w.durchgefallen = f(msg, state, speicherGeht, "", () => {},
      (m) => w.geladen.push(m), () => { w.gerendert++; }) === "durchgefallen";
    return w;
  }

  const spielt = { match: { id: 1 }, pendingResult: { matchId: 1 } };

  let r = lauf({ type: "reload", marke: "neu1" }, spielt);
  p("B1 Befehl lädt auch mit laufendem Spiel", r.geladen, ["neu1"]);
  p("B1 Befehl beendet die Nachrichten-Kette", r.durchgefallen, false);

  r = lauf({ type: "reload" }, { match: null, pendingResult: null });
  p("B2 Befehl ohne Marke: lädt trotzdem", r.geladen, [""]);

  // B8: Ohne nutzbaren localStorage überlebt weder der Spielstand noch die
  // Geräte-Kennung das Neuladen — der Server zeigte danach „Feld belegt".
  // Dort darf der Befehl NICHT springen, sondern nur den Hinweis zeigen.
  r = lauf({ type: "reload", marke: "neu1" }, spielt, false);
  p("B8 ohne Speicher + laufendes Spiel: lädt NICHT", r.geladen, []);
  p("B8 ohne Speicher: zeigt stattdessen den Hinweis", r.gerendert, 1);

  r = lauf({ type: "reload", marke: "neu1" }, { match: null, pendingResult: null }, false);
  p("B8 ohne Speicher, aber nichts zu verlieren: lädt", r.geladen, ["neu1"]);

  r = lauf({ type: "pong", marke: "x" }, spielt);
  p("fremde Nachricht: unberührt", r.geladen, []);
  p("fremde Nachricht: fällt durch", r.durchgefallen, true);
}

{
  // B6: Der Knopf erscheint nur, wenn der Turnier-PC den Befehl kennt. Der
  // Relay wird bei jedem main-Merge deployt, die App erst mit einem
  // Release-Tag — dazwischen spricht diese Seite womöglich mit einem älteren
  // Host, und der verwirft eine unbekannte Aktion STILL.
  const src = abschnitt(tl, "function renderTabletsReload() {", "async function reloadTablets()", "TL-Render");
  const knopf = { hidden: null };
  const render = new Function("state", "$", src + "\nreturn renderTabletsReload;");

  render({ can_reload_tablets: true }, () => knopf)();
  p("B6 neuer Host: Knopf sichtbar", knopf.hidden, false);

  render({}, () => knopf)();
  p("B6 alter Host: Knopf verborgen", knopf.hidden, true);

  render({ can_reload_tablets: false }, () => knopf)();
  p("B6 Host sagt nein: Knopf verborgen", knopf.hidden, true);
}

{
  const src = abschnitt(tl, "async function reloadTablets() {", "async function toggleAuto() {", "TL-Befehl");
  const bauen = new Function("confirm", "send", src + "\nreturn reloadTablets;");

  let gesendet = [];
  const mach = (antwort) => {
    gesendet = [];
    return bauen(() => antwort, (opId, payload) => { gesendet.push([opId, payload]); })();
  };

  await mach(false);
  p("B4 Rückfrage abgelehnt: kein Befehl", gesendet, []);

  await mach(true);
  p("B4 bestätigt: genau ein Befehl", gesendet.length, 1);
  p("B4 richtige Aktion", gesendet[0][1], { action: "reload_tablets" });

  // B5: Zwei bewusste Drücke sind zwei Befehle. Die Vorgangs-Kennung trägt
  // die Idempotenz im Host — wiederholte sie sich, schluckte er den zweiten
  // als Doppeltipp, etwa wenn beim ersten Mal ein Tablet offline war.
  const erste = gesendet[0][0];
  await new Promise((r) => setTimeout(r, 3));
  await mach(true);
  p("B5 zweiter Druck: neue Vorgangs-Kennung", gesendet[0][0] !== erste, true);
}

if (fehler) {
  console.error(`\n${fehler} Fehler.`);
  process.exit(1);
}
console.log("\nVersionsabgleich + Fernbefehl: alle Fälle korrekt.");
