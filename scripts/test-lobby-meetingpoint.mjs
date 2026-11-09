// Meetingpoint-Eintrag in der Kiosk-Feldwahl (Spec meetingpoint-am-kiosk-tablet):
// An/Aus aus dem HTTP-Status, Hinweis-Knopf aus dem Hüllen-Merker, Marke
// zurücksetzen ohne die gemerkte Halle zu verlieren.
import * as m from "../src/io/lobbyMeetingpoint.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

ok("Ziel ohne Halle (Merker der Hülle greift)", m.meetingpointZiel(), "/anzeige?layout=meetingpoint");
ok("Schlüssel wie in anzeige.html", m.ZULETZT_KEY, "badhub.anzeige.zuletzt");

ok("200 → an", m.mpAusStatus(200, false), true);
ok("404 → aus", m.mpAusStatus(404, true), false);
ok("Netzfehler hält an", m.mpAusStatus(0, true), true);
ok("Netzfehler hält aus", m.mpAusStatus(0, false), false);
ok("500 hält an", m.mpAusStatus(500, true), true);
ok("304/sonstiges hält aus", m.mpAusStatus(302, false), false);
ok("undefined-Start → aus", m.mpAusStatus(undefined, undefined), false);

const mp = JSON.stringify({ layout: "meetingpoint", court: null, halle: "Halle 2" });
ok("Marke erkannt", m.warZuletztMeetingpoint(mp), true);
ok("null → keine Marke", m.warZuletztMeetingpoint(null), false);
ok("leer → keine Marke", m.warZuletztMeetingpoint(""), false);
ok("kaputtes JSON → keine Marke", m.warZuletztMeetingpoint("{nicht json"), false);
ok("JSON-null → keine Marke", m.warZuletztMeetingpoint("null"), false);
ok("Array → keine Marke", m.warZuletztMeetingpoint('["meetingpoint"]'), false);
ok("Zahl → keine Marke", m.warZuletztMeetingpoint("42"), false);
ok("Zähltafel → keine Marke", m.warZuletztMeetingpoint(JSON.stringify({ layout: "tafel", court: 3 })), false);

ok("Marke weg, Halle bleibt", JSON.parse(m.ohneMeetingpointMarke(mp)), { layout: "tafel", court: null, halle: "Halle 2" });
ok("Feld bleibt", JSON.parse(m.ohneMeetingpointMarke(JSON.stringify({ layout: "meetingpoint", court: 7 }))),
  { layout: "tafel", court: 7, halle: null });
ok("danach keine Marke mehr", m.warZuletztMeetingpoint(m.ohneMeetingpointMarke(mp)), false);
ok("nichts zu tun bei Zähltafel", m.ohneMeetingpointMarke(JSON.stringify({ layout: "tafel", court: 3 })), null);
ok("nichts zu tun bei null", m.ohneMeetingpointMarke(null), null);
ok("nichts zu tun bei kaputtem JSON", m.ohneMeetingpointMarke("{x"), null);

if (failures) { console.error(`\n${failures} Fehler`); process.exit(1); }
console.log("\nalle Prüfungen bestanden");
