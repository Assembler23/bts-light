// Testet den Turnier-GUID-Anhang an Live-Links (src/io/liveLink.mjs) — die
// Frontend-Schwester von `src-tauri/src/aushang.rs::link_mit_guid`.
//
// Der Online-Monitor (CourtMonitorPanel) baut seine Links direkt aus der
// Verbands-`live_url`. Ohne `&g=` zeigen sie bei zwei parallelen BVBB-
// Turnieren auf den falschen Stand (Review-Befund I3).
import { linkMitGuid } from "../src/io/liveLink.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const GUID = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA";

ok(
  "hängt ohne bestehende Query mit ? an",
  linkMitGuid("https://badhub.de/live?t=bvbb", GUID),
  `https://badhub.de/live?t=bvbb&g=${GUID}`,
);
ok(
  "hängt ohne Query mit ? an",
  linkMitGuid("https://badhub.de/live", GUID),
  `https://badhub.de/live?g=${GUID}`,
);
ok(
  "ohne GUID bleibt die Adresse unverändert",
  linkMitGuid("https://badhub.de/live?t=bvbb", undefined),
  "https://badhub.de/live?t=bvbb",
);
ok(
  "ohne GUID (leerer String) bleibt unverändert",
  linkMitGuid("https://badhub.de/live?t=bvbb", ""),
  "https://badhub.de/live?t=bvbb",
);
// Leere Adresse bleibt leer — der Aufrufer meldet „keine Live-Seite".
ok("leere Adresse bleibt leer", linkMitGuid("", GUID), "");
ok("Adresse wird getrimmt", linkMitGuid("  https://badhub.de/live  ", GUID), `https://badhub.de/live?g=${GUID}`);
// Reihenfolge: g= kommt VOR &display=/&halle=, weil das Panel die GUID vor
// diesen Parametern anhängt (siehe CourtMonitorPanel.tsx).
ok(
  "g= steht vor nachträglich angehängtem display=",
  linkMitGuid("https://badhub.de/live?t=bvbb", GUID) + "&display=monitor",
  `https://badhub.de/live?t=bvbb&g=${GUID}&display=monitor`,
);

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("\nAlle Tests bestanden");
