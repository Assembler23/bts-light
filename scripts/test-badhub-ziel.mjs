// Testet den Produktiv-/Test-Umschalter (src/io/badhubZiel.mjs) — die
// Frontend-Schwester von `src-tauri/src/badhub_host.rs`.
//
// Beide Richtungen sind heikel: Greift die Umschaltung zu weit, verliert ein
// Betreiber mit eigener badhub-Instanz beim ersten Klick seine Adresse. Greift
// sie zu kurz, läuft ein Testturnier stumm in die Produktiv-Datenbank — der
// teure Fehlerfall, denn er fällt erst auf, wenn die falschen Daten öffentlich
// im Liveticker stehen.
import {
  BADHUB_HOST_TEST,
  badhubHostFuer,
  badhubUrlFuer,
  badhubZielFuer,
  istTestsystem,
  istUmschaltbar,
} from "../src/io/badhubZiel.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const PUSH_LIVE = "https://badhub.de/api/live_update.php";
const PUSH_TEST = "https://test.badhub.de/api/live_update.php";

// ── Hin und zurück ──

ok("Push-URL auf Test", badhubUrlFuer(PUSH_LIVE, true), PUSH_TEST);
ok("Push-URL zurück auf Produktiv", badhubUrlFuer(PUSH_TEST, false), PUSH_LIVE);
// Zweimal dasselbe Ziel darf nichts verdoppeln.
ok("Test bleibt Test", badhubUrlFuer(PUSH_TEST, true), PUSH_TEST);
ok("Produktiv bleibt Produktiv", badhubUrlFuer(PUSH_LIVE, false), PUSH_LIVE);

// ── Pfad und Query der Live-Seite bleiben erhalten ──

ok(
  "Live-Seite mit Query",
  badhubUrlFuer("https://badhub.de/live?t=bvbb", true),
  "https://test.badhub.de/live?t=bvbb",
);
ok(
  "Teilnehmer-Pfad zurück",
  badhubUrlFuer("https://test.badhub.de/live/bvbb/teilnehmer", false),
  "https://badhub.de/live/bvbb/teilnehmer",
);
ok(
  "www zählt als Produktivsystem",
  badhubUrlFuer("https://www.badhub.de/live?t=bvbb", true),
  "https://test.badhub.de/live?t=bvbb",
);

// ── Fremde Hosts bleiben unangetastet ──

for (const url of [
  "https://liveticker.example.org/api/live_update.php",
  "http://192.168.1.50/api/live_update.php",
  "https://badhub.example.com/live?t=x",
  "kein-url",
  "",
]) {
  ok(`fremd bleibt fremd (${url || "leer"})`, badhubUrlFuer(url, true), url);
}

// ── Erkennung des aktiven Systems ──

ok("Test erkannt", istTestsystem(PUSH_TEST), true);
ok("Test erkannt trotz Großschreibung", istTestsystem("https://TEST.BADHUB.DE/live?t=x"), true);
ok("Produktiv erkannt", istTestsystem(PUSH_LIVE), false);
ok("www ist nicht Test", istTestsystem("https://www.badhub.de/live?t=x"), false);
// Ein fremder Host ist kein Testlauf, auch wenn „test" darin vorkommt.
ok("fremder test-Host zählt nicht", istTestsystem("https://test.example.org/api/live_update.php"), false);
ok("Unsinn ist kein Test", istTestsystem("unsinn"), false);
ok("undefined ist kein Test", istTestsystem(undefined), false);

// ── Was der Schalter überhaupt anfassen darf ──
// Ohne diese Frage bietet die Oberfläche einen Schalter an, der bei einer
// eigenen badhub-Instanz wirkungslos zurückspringt (Review 25.08.2026).
ok("badhub.de ist umschaltbar", istUmschaltbar("https://badhub.de/api/live_update.php"), true);
ok("test.badhub.de ist umschaltbar", istUmschaltbar(PUSH_TEST), true);
ok("www ist umschaltbar", istUmschaltbar("https://www.badhub.de/live?t=x"), true);
ok("eigene Instanz nicht", istUmschaltbar("https://liveticker.example.org/api/live_update.php"), false);
ok("IP nicht", istUmschaltbar("http://192.168.1.50/api/live_update.php"), false);
ok("Unsinn nicht", istUmschaltbar("kein-url"), false);

ok("Host produktiv", badhubHostFuer(false), "badhub.de");
ok("Host Test", badhubHostFuer(true), BADHUB_HOST_TEST);

// ── Kompletter Zugang: Passwort bleibt unberührt ──

const zugang = { url: PUSH_LIVE, password: "geheim", live_url: "https://badhub.de/live?t=bvbb" };
const test = badhubZielFuer(zugang, true);
ok("Zugang Push-URL", test.url, PUSH_TEST);
ok("Zugang Live-URL", test.live_url, "https://test.badhub.de/live?t=bvbb");
ok("Zugang Passwort unberührt", test.password, "geheim");
// Ein leerer Live-Link bleibt leer statt zu einer nackten Host-URL zu werden.
ok("leere Live-URL bleibt leer", badhubZielFuer({ url: PUSH_LIVE, live_url: "" }, true).live_url, "");

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("\nAlle Tests bestanden");
