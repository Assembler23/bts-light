// Testet die Regel aus scripts/check-version-tagged.mjs — ohne git, ohne Netz.
//
// Die wichtigsten Tests sind die beiden Grenz-Bloecke. Zwei Entwuerfe dieses
// Checks haetten den echten Vorfall verschluckt, beides gemessen statt geraten:
//   1. "ist die aktuelle Version getaggt und wie alt?" - 0.9.199 war zur
//      Meldung 1,4 h alt, weil jede Version von der naechsten ueberschrieben
//      wird, bevor eine Uhr greift.
//   2. "wie alt ist der aelteste unveroeffentlichte Commit?" - der war 19 h alt,
//      unter der 24-h-Grenze. Zwoelf Spruenge in 19 Stunden.
// Deshalb zwei Maszstaebe: Uhr UND Menge. Beim Vorfall greift nur die Menge.
import {
  bewerte,
  GRENZE_SPRUENGE,
  cargoVersion,
  tauriVersion,
  istAuslieferbar,
  GRENZE_STUNDEN,
} from "./check-version-tagged.mjs";

let failures = 0;
function eq(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const c = (alterStunden, sha = "abc1234", titel = "Feature") => ({ sha, titel, alterStunden });
const basis = { cargo: "0.9.199", tauri: "0.9.199", letzterTag: "v0.9.187", offen: [] };
const u = (o) => bewerte({ ...basis, ...o });

// ── Nichts offen ───────────────────────────────────────────────────────────
eq("alles geliefert → ok", u({ offen: [] }).ok, true);
eq("… code", u({ offen: [] }).code, "alles-geliefert");

// ── Altersgrenze ───────────────────────────────────────────────────────────
// offen ist "neueste zuerst", der aelteste steht hinten.
eq("frisch gemergt (1 h) → ok", u({ offen: [c(1)] }).ok, true);
eq("… code", u({ offen: [c(1)] }).code, "frisch");
eq("kurz vor der Grenze → ok", u({ offen: [c(GRENZE_STUNDEN - 0.1)] }).ok, true);
eq("genau auf der Grenze → rot", u({ offen: [c(GRENZE_STUNDEN)] }).ok, false);
eq("… code", u({ offen: [c(50)] }).code, "release-faellig");

// Entscheidend: es zaehlt der AELTESTE, nicht der neueste.
const gemischt = u({ offen: [c(0.5, "neu1111", "frisch gemergt"), c(48, "alt2222", "liegt seit zwei Tagen")] });
eq("neuer Commit rettet einen alten NICHT", gemischt.ok, false);
eq("… nennt die Anzahl", gemischt.text.includes("2 auslieferbare"), true);
eq("… nennt das Alter in Tagen", gemischt.text.includes("2 Tag(e) alt"), true);
eq("… nennt den aeltesten Commit", gemischt.text.includes("alt2222"), true);
eq("… nennt den fertigen Befehl", gemischt.text.includes("git push origin v0.9.199"), true);
eq("… nennt die Folge", gemischt.text.includes("Turnier-PC"), true);

// ── Mengen-Grenze: faengt, was die Uhr verschluckt ─────────────────────────
// Der zweite Entwurf pruefte nur das Alter und schwieg beim echten Vorfall
// (aeltester Commit 19 h, Grenze 24 h). Ein Stapel bildet sich schneller als
// eine Uhr ihn bemerkt.
eq("wenige Spruenge, frisch → ok", u({ offen: [c(2)], spruenge: ["0.9.199"] }).ok, true);
eq(
  "vier Spruenge, frisch → noch ok",
  u({ offen: [c(2)], spruenge: ["0.9.196", "0.9.197", "0.9.198", "0.9.199"] }).ok,
  true,
);
const grenzeErreicht = u({
  offen: [c(2)],
  spruenge: ["0.9.195", "0.9.196", "0.9.197", "0.9.198", "0.9.199"],
});
eq(`${GRENZE_SPRUENGE} Spruenge, frisch → rot (Menge schlaegt Uhr)`, grenzeErreicht.ok, false);
eq("… Begruendung nennt die Spruenge", grenzeErreicht.text.includes("unveroeffentlichte Versionsspruenge"), true);
// Und umgekehrt: die Uhr allein reicht weiter aus.
const nurUhr = u({ offen: [c(30)], spruenge: ["0.9.199"] });
eq("ein Sprung, aber 30 h alt → rot", nurUhr.ok, false);
eq("… Begruendung nennt das Alter", nurUhr.text.includes("Tag(e) alt"), true);

// ── Der echte Vorfall, nachgestellt ────────────────────────────────────────
// 0.9.188 am 11.08. gemergt, danach elf weitere Spruenge, Meldung am 13.08.
// Der erste Entwurf haette geschwiegen (aktuelle Version 1,4 h alt) — diese
// Regel meldet ab dem zweiten Tag.
const vorfall = u({
  letzterTag: "v0.9.187",
  // Gemessen: 16 auslieferbare Commits, aeltester 19 h, zwoelf Spruenge.
  offen: [c(1.4, "c4d9705", "Hebel D (v0.9.199)"), c(12, "115b704", "A2 Reconnect (v0.9.197)"), c(19, "0a5a67a", "TL-Web Verein (v0.9.188)")],
  spruenge: ["0.9.199","0.9.198","0.9.197","0.9.196","0.9.195","0.9.194","0.9.193","0.9.192","0.9.191","0.9.190","0.9.189","0.9.188"],
});
eq("Vorfall → rot", vorfall.ok, false);
eq("Vorfall: Uhr allein haette geschwiegen", u({ letzterTag:"v0.9.187", offen:[c(19)] }).ok, true);
eq("Vorfall nennt die offenen Commits", vorfall.text.includes("3 auslieferbare"), true);

// ── Uneinige Versionsdateien (Pruefung 2 des Release-Ablaufs) ──────────────
const uneinig = u({ tauri: "0.9.198" });
eq("Cargo != tauri.conf → rot", uneinig.ok, false);
eq("… code", uneinig.code, "versionen-uneinig");
eq("… nennt beide Werte", uneinig.text.includes("0.9.199") && uneinig.text.includes("0.9.198"), true);
// Uneinigkeit schlaegt "alles geliefert": ein Tag auf widerspruechliche Dateien
// waere wertlos, das muss auffallen, auch wenn gerade nichts offen ist.
eq("Uneinigkeit schlaegt alles-geliefert", u({ tauri: "0.9.198", offen: [] }).ok, false);

// ── Fehlende Angaben ───────────────────────────────────────────────────────
eq("Cargo-Version fehlt → rot", u({ cargo: null }).ok, false);
eq("tauri-Version fehlt → rot", u({ tauri: null }).ok, false);
eq("… code", u({ cargo: null }).code, "version-fehlt");
// Repo ohne jeden Tag: alles offen, Regel greift trotzdem.
eq("kein Tag vorhanden + alt → rot", u({ letzterTag: null, offen: [c(72)] }).ok, false);
eq("kein Tag vorhanden + frisch → ok", u({ letzterTag: null, offen: [c(2)] }).ok, true);

// ── istAuslieferbar: was macht ein Release noetig? ─────────────────────────
eq("src/ zaehlt", istAuslieferbar("src/App.tsx"), true);
eq("src-tauri/src zaehlt", istAuslieferbar("src-tauri/src/main.rs"), true);
eq("Cargo.toml zaehlt", istAuslieferbar("src-tauri/Cargo.toml"), true);
eq("docs/ zaehlt nicht", istAuslieferbar("docs/release.md"), false);
eq("Markdown zaehlt nicht", istAuslieferbar("README.md"), false);
eq("ADR zaehlt nicht", istAuslieferbar("docs/adr/0021-foo.md"), false);
eq("Workflow zaehlt nicht", istAuslieferbar(".github/workflows/ci.yml"), false);
eq("Rust-Integrationstest zaehlt nicht", istAuslieferbar("src-tauri/tests/btp_probe.rs"), false);
eq("test-Skript zaehlt nicht", istAuslieferbar("scripts/test-gamepoint.mjs"), false);
// CI-Helfer zaehlen nicht — sonst startet dieser Check die Uhr fuer sich selbst.
eq("check-Skript zaehlt nicht", istAuslieferbar("scripts/check-version-tagged.mjs"), false);
// Ein normales Skript SCHON — build-release-page.mjs wirkt auf die Auslieferung.
eq("normales Skript zaehlt", istAuslieferbar("scripts/build-release-page.mjs"), true);

// ── Parser ─────────────────────────────────────────────────────────────────
eq(
  "cargoVersion nimmt die erste version-Zeile",
  cargoVersion('[package]\nname = "bts"\nversion = "0.9.199"\n\n[dependencies]\nfoo = { version = "1.0" }'),
  "0.9.199",
);
eq("cargoVersion ohne Treffer → null", cargoVersion('[package]\nname = "bts"'), null);
eq("tauriVersion liest JSON", tauriVersion('{"productName":"BTS","version":"0.9.199"}'), "0.9.199");
eq("tauriVersion bei kaputtem JSON → null", tauriVersion("{nicht json"), null);

if (failures > 0) {
  console.error(`\n✗ Release-Faellig-Test: ${failures} Fehler.`);
  process.exit(1);
}
console.log("\n✓ Release-Faellig-Test: Regel, Altersgrenze und Auslieferbarkeit ok.");
