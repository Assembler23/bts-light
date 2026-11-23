// Testet die Regel aus scripts/check-release-freigabe.mjs — ohne git, ohne Netz.
//
// Der Workflow „Release auslösen" taggt mit einem Schlüssel, der das
// Tag-Ruleset umgehen darf. Diese Regel ist alles, was zwischen einem Klick
// und einem Auto-Update auf allen Turnier-PCs steht — jeder Hinderungsgrund
// bekommt deshalb einen eigenen Test, und jeder Lesefehler muss sperren.
import { readFileSync } from "node:fs";
import { pruefeFreigabe, paketVersion, ciUeberspringt, waehleCi, CI_PATHS_IGNORE } from "./check-release-freigabe.mjs";

let failures = 0;
function eq(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const gut = {
  ref: "refs/heads/main",
  sha: "aaa",
  mainSpitze: "aaa",
  eingabe: "0.9.422",
  cargo: "0.9.422",
  tauri: "0.9.422",
  paket: "0.9.422",
  tagVorhanden: false,
  ci: { status: "completed", conclusion: "success", headSha: "aaa" },
};
const u = (o = {}) => pruefeFreigabe({ ...gut, ...o });
const sperrt = (name, o, stichwort) => {
  const r = u(o);
  eq(`${name} → gesperrt`, r.ok, false);
  eq(`${name} → kein Tag`, r.tag, null);
  eq(`${name} → Grund genannt`, r.fehler.some((f) => f.includes(stichwort)), true);
};

// ── Der gute Fall ──────────────────────────────────────────────────────────
eq("alles passt → ok", u().ok, true);
eq("… Tag", u().tag, "v0.9.422");
eq("… keine Fehler", u().fehler.length, 0);
eq("Bestätigung mit v davor", u({ eingabe: "v0.9.422" }).ok, true);
eq("Bestätigung mit Leerzeichen", u({ eingabe: " 0.9.422 " }).ok, true);

// ── Jeder Hinderungsgrund sperrt für sich ──────────────────────────────────
sperrt("Branch statt main", { ref: "refs/heads/feature" }, "main gestartet");
sperrt("Tag als Ref", { ref: "refs/tags/v0.9.422" }, "main gestartet");
sperrt("main weitergelaufen", { mainSpitze: "bbb" }, "bewegt");
sperrt("main nicht lesbar", { mainSpitze: null }, "bewegt");
sperrt("Cargo abweichend", { cargo: "0.9.421" }, "uneinig");
sperrt("tauri.conf abweichend", { tauri: "0.9.421" }, "uneinig");
sperrt("package.json abweichend", { paket: "0.9.421" }, "uneinig");
sperrt("Version fehlt", { paket: null }, "nicht lesbar");
sperrt(
  "Version mit Zusatz",
  { cargo: "0.9.422-rc1", tauri: "0.9.422-rc1", paket: "0.9.422-rc1", eingabe: "0.9.422-rc1" },
  "X.Y.Z",
);
sperrt("falsche Bestätigung", { eingabe: "0.9.421" }, "Bestätigung");
sperrt("leere Bestätigung", { eingabe: "" }, "Bestätigung");
sperrt("Tag existiert", { tagVorhanden: true }, "existiert bereits");
sperrt("kein CI-Lauf", { ci: null }, "Kein CI-Lauf");
sperrt("CI läuft noch", { ci: { status: "in_progress", conclusion: null, headSha: "aaa" } }, "läuft noch");
sperrt("CI rot", { ci: { status: "completed", conclusion: "failure", headSha: "aaa" } }, "nicht grün");
sperrt("CI abgebrochen", { ci: { status: "completed", conclusion: "cancelled", headSha: "aaa" } }, "nicht grün");

// ── Der CI-Lauf muss zu DIESEM Commit gehören ──────────────────────────────
// Das Fenster, das der Review fand: Merge, sofort auslösen, der Lauf für den
// neuen Commit existiert noch nicht — die Abfrage liefert den grünen Vorgänger.
const alt = { status: "completed", conclusion: "success", headSha: "alt" };
sperrt("grüner Lauf des Vorgängers, seither Code", { ci: alt, seitCi: ["package.json", "docs/x.md"] }, "noch keinen CI-Lauf");
sperrt("… Abstand nicht ermittelbar", { ci: alt, seitCi: null }, "nicht ermittelbar");
sperrt("… Abstand nicht übergeben", { ci: alt }, "nicht ermittelbar");
eq("älterer Lauf, seither nur Doku → ok", u({ ci: alt, seitCi: ["docs/release.md", "README.md"] }).ok, true);
eq("älterer Lauf, seither nichts → ok", u({ ci: alt, seitCi: [] }).ok, true);
eq("Workflow-Datei zählt als Code", u({ ci: alt, seitCi: [".github/workflows/ci.yml"] }).ok, false);

// Die Ausnahme oben stimmt nur, solange sie ci.yml nachzeichnet.
const ciYml = readFileSync(".github/workflows/ci.yml", "utf8");
const block = ciYml.match(/paths-ignore:\n((?:\s+- .+\n)+)/)?.[1] ?? "";
const ausCi = [...block.matchAll(/- ['"]?([^'"\n]+)['"]?/g)].map((m) => m[1]);
eq("paths-ignore in ci.yml = CI_PATHS_IGNORE", JSON.stringify(ausCi), JSON.stringify(CI_PATHS_IGNORE));
for (const [p, w] of [
  ["docs/release.md", true], ["README.md", true], ["src/x/NOTIZ.md", true], ["LICENSE", true],
  [".github/ISSUE_TEMPLATE/bug.yml", true], [".github/workflows/ci.yml", false],
  ["scripts/check-release-freigabe.mjs", false], ["src-tauri/Cargo.toml", false], ["docs", false],
]) eq(`ciUeberspringt(${p})`, ciUeberspringt(p), w);

// ── Hand-Lauf, wenn der Merge keinen Push-Lauf auslöste ────────────────────
// Dependabot-Automerge pusht mit dem GITHUB_TOKEN: Für die Spitze von main
// entsteht nie ein Push-Lauf (so hing v0.9.473 am 23.11.2026).
{
  const push = { status: "completed", conclusion: "success", headSha: "alt" };
  const hand = { status: "completed", conclusion: "success", headSha: "aaa" };
  eq("Push-Lauf für den Commit gewinnt", waehleCi({ ...push, headSha: "aaa" }, hand, "aaa").headSha, "aaa");
  eq("Hand-Lauf für den Commit zählt", waehleCi(push, hand, "aaa"), hand);
  eq("Hand-Lauf für fremden Commit zählt nicht", waehleCi(push, { ...hand, headSha: "x" }, "aaa"), push);
  eq("kein Hand-Lauf → Push-Lauf", waehleCi(push, null, "aaa"), push);
  eq("gar kein Lauf → null", waehleCi(null, null, "aaa"), null);
  eq("nur fremder Hand-Lauf → null", waehleCi(null, { ...hand, headSha: "x" }, "aaa"), null);
  eq("leerer Commit greift nie zum Hand-Lauf", waehleCi(null, { ...hand, headSha: "" }, ""), null);
  eq("Hand-Lauf grün → Freigabe", u({ ci: waehleCi(push, hand, "aaa") }).ok, true);
  eq("Hand-Lauf rot → gesperrt", u({ ci: waehleCi(push, { ...hand, conclusion: "failure" }, "aaa") }).ok, false);
  eq("ci.yml ist von Hand startbar", /^  workflow_dispatch:\s*$/m.test(ciYml), true);
}

// ── Alle Gründe auf einmal, nicht nur der erste ────────────────────────────
eq(
  "mehrere Gründe werden gesammelt",
  u({ ref: "refs/heads/x", tagVorhanden: true, ci: null }).fehler.length,
  3,
);

// ── Hilfsfunktion ──────────────────────────────────────────────────────────
eq("paketVersion liest", paketVersion('{"version":"1.2.3"}'), "1.2.3");
eq("paketVersion bei Müll", paketVersion("{kaputt"), null);

if (failures > 0) {
  console.error(`\n${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("\nAlle Tests grün");
