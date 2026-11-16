// Prüft das Bau-Skript des Seiten-Pakets (Spec hotfix-ohne-neustart).
//
// Warum: Das Paket wird mit dem Updater-Schlüssel signiert und läuft danach
// auf jedem Turnier-PC. Die Regeln davor — welcher Zweig überhaupt ein Paket
// ergibt, welche Seiten drin sind, was der Index nennt — dürfen nicht still
// aufweichen.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, symlinkSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

import {
  KERN_VERSIONEN_IM_INDEX,
  MAX_FOLGE,
  bericht,
  geaenderteKernPfade,
  geaenderteSeiten,
  indexAusListe,
  istKernPfad,
  kernAusTag,
  nutzlast,
  paketName,
  pfadeAusDiffZ,
  plan,
  pruefeSeitenModi,
  seitenAusAssetsRs,
  vergleicheVersion,
} from "./build-seiten-paket.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let anzahl = 0;
function pruefe(name, fn) {
  fn();
  anzahl++;
  console.log(`✓ ${name}`);
}

pruefe("Kern-Pfade: alles unter src-tauri außer den Seiten, dazu relay-proto und Cargo", () => {
  for (const p of [
    "src-tauri/src/tablet/server.rs",
    "src-tauri/Cargo.toml",
    "src-tauri/tauri.conf.json",
    "src-tauri/build.rs",
    "src-tauri/assets/flags/GER.svg",
    "src-tauri/assets/unter/tablet.html",
    "src-tauri/assets/tablet.html.bak",
    "relay-proto/src/lib.rs",
    "Cargo.toml",
    "Cargo.lock",
    "src-tauri\\src\\lib.rs",
  ]) {
    assert.equal(istKernPfad(p), true, p);
  }
  for (const p of [
    "src-tauri/assets/tablet.html",
    "src-tauri/assets/stream-werbung.html",
    "docs/changelog.md",
    "src/io/akku.mjs",
    "scripts/test-akku.mjs",
    "relay/src/main.rs",
    "package.json",
  ]) {
    assert.equal(istKernPfad(p), false, p);
  }
});

pruefe("Kern-Pfad geändert → kein Paket; nur HTML → Paket", () => {
  assert.deepEqual(
    geaenderteKernPfade(["src-tauri/assets/tablet.html", "docs/tablet.md", ""]),
    [],
  );
  assert.deepEqual(
    geaenderteKernPfade(["src-tauri/assets/tablet.html", "src-tauri/src/sync.rs", "Cargo.lock"]),
    ["src-tauri/src/sync.rs", "Cargo.lock"],
  );
  assert.deepEqual(
    geaenderteSeiten(["src-tauri/assets/tl.html", "docs/x.md", "src-tauri/assets/flags/a.html"]),
    ["src-tauri/assets/tl.html"],
  );
});

pruefe("Tag → Kern-Version, nur vX.Y.Z", () => {
  assert.equal(kernAusTag("v0.9.421"), "0.9.421");
  for (const falsch of ["0.9.421", "v0.9", "v0.9.421-rc1", "hotfix/v0.9.421", "", undefined]) {
    assert.throws(() => kernAusTag(falsch), /Kein Release-Tag/, String(falsch));
  }
});

const assetsRs = readFileSync(join(root, "src-tauri/src/tablet/assets.rs"), "utf8");
const seitenNamen = seitenAusAssetsRs(assetsRs);

pruefe("Seitenliste aus assets.rs = die *.html im Assets-Ordner (Drift)", () => {
  const aufPlatte = readdirSync(join(root, "src-tauri/assets"))
    .filter((n) => n.endsWith(".html"))
    .sort();
  assert.deepEqual([...seitenNamen].sort(), aufPlatte);
  assert.ok(seitenNamen.includes("tablet.html"));
  assert.throws(() => seitenAusAssetsRs("fn main() {}"), /Keine Seiten/);
  assert.throws(
    () => seitenAusAssetsRs('    A => "a.html", [];\n    B => "a.html", [];\n'),
    /doppelt/,
  );
});

pruefe("Nutzlast: gzip-JSON mit allen Feldern, die die App prüft", () => {
  const seiten = Object.fromEntries(seitenNamen.map((n) => [n, `<p>${n}</p>`]));
  const bytes = nutzlast({
    kern: "0.9.421",
    folge: 2,
    commit: "abc123",
    gebaut: "2026-11-16T14:03:00Z",
    seiten,
  });
  assert.equal(bytes[0], 0x1f);
  assert.equal(bytes[1], 0x8b);
  const inhalt = JSON.parse(gunzipSync(bytes).toString("utf8"));
  assert.equal(inhalt.format, 1);
  assert.equal(inhalt.art, "seiten");
  assert.equal(inhalt.kern, "0.9.421");
  assert.equal(inhalt.folge, 2);
  assert.equal(inhalt.commit, "abc123");
  assert.deepEqual(Object.keys(inhalt.seiten).sort(), [...seitenNamen].sort());
});

pruefe("Nutzlast lehnt Folge 0, krumme Version und leere Seite ab", () => {
  const gut = { kern: "1.2.3", folge: 1, commit: "", gebaut: "", seiten: { "a.html": "x" } };
  assert.throws(() => nutzlast({ ...gut, folge: 0 }), /Folge/);
  assert.throws(() => nutzlast({ ...gut, folge: 1.5 }), /Folge/);
  assert.throws(() => nutzlast({ ...gut, folge: NaN }), /Folge/);
  assert.throws(() => nutzlast({ ...gut, kern: "v1.2.3" }), /Kern-Version/);
  assert.throws(() => nutzlast({ ...gut, seiten: { "a.html": "  \n" } }), /leer/);
});

pruefe("Versionsvergleich zählt Zahlen, nicht Zeichen", () => {
  assert.ok(vergleicheVersion("0.9.100", "0.9.99") > 0);
  assert.ok(vergleicheVersion("0.10.0", "0.9.999") > 0);
  assert.equal(vergleicheVersion("1.2.3", "1.2.3"), 0);
});

pruefe("Diff-Liste mit -z: Sonderzeichen bleiben Pfade, keine Anführungszeichen", () => {
  assert.deepEqual(pfadeAusDiffZ("docs/a.md\0src-tauri/src/ü.rs\0"), [
    "docs/a.md",
    "src-tauri/src/ü.rs",
  ]);
  assert.deepEqual(geaenderteKernPfade(pfadeAusDiffZ("src-tauri/src/ü.rs\0")), [
    "src-tauri/src/ü.rs",
  ]);
  // So sähe es OHNE -z aus — und fiele durch die Regel:
  assert.equal(istKernPfad('"src-tauri/src/\\303\\274.rs"'), false);
});

pruefe("Seiten müssen gewöhnliche Dateien sein (kein Verweis, kein Untermodul)", () => {
  const zeile = (modus, art, name) => `${modus} ${art} ${"a".repeat(40)}\tsrc-tauri/assets/${name}`;
  const gut = [zeile("100644", "blob", "a.html"), zeile("100644", "blob", "b.html")].join("\0");
  pruefeSeitenModi(gut, ["a.html", "b.html"]);
  assert.throws(
    () => pruefeSeitenModi(zeile("120000", "blob", "a.html"), ["a.html"]),
    /keine gewöhnliche Datei \(120000 blob\)/,
  );
  assert.throws(
    () => pruefeSeitenModi(zeile("160000", "commit", "a.html"), ["a.html"]),
    /keine gewöhnliche Datei/,
  );
  assert.throws(() => pruefeSeitenModi(zeile("100755", "blob", "a.html"), ["a.html"]), /100755/);
  assert.throws(() => pruefeSeitenModi(gut, ["a.html", "c.html"]), /c\.html.*fehlt/);
});

pruefe("Index aus der Server-Liste: nur Pakete mit Signatur, höchste Folge, drei Versionen", () => {
  assert.deepEqual(indexAusListe([]), { format: 1, pakete: [] });
  const liste = [
    "seiten-0.9.421-1.btsp", "seiten-0.9.421-1.btsp.sig",
    "seiten-0.9.421-3.btsp", "seiten-0.9.421-3.btsp.sig",
    "seiten-0.9.421-2.btsp", "seiten-0.9.421-2.btsp.sig",
    // Ohne Signatur zählt ein Paket nicht — auch nicht als „neuestes".
    "seiten-0.9.421-9.btsp",
    "seiten-0.9.430-1.btsp", "seiten-0.9.430-1.btsp.sig",
    "seiten-0.9.99-5.btsp", "seiten-0.9.99-5.btsp.sig",
    "seiten-0.10.0-1.btsp", "seiten-0.10.0-1.btsp.sig",
    // Unfug wird übersehen, nicht geglaubt.
    "seiten-0.9.500-0.btsp", "seiten-0.9.500-0.btsp.sig",
    "seiten-0.9.500-007.btsp", "seiten-0.9.500-007.btsp.sig",
    "seiten-0.9.500-9999999.btsp", "seiten-0.9.500-9999999.btsp.sig",
    "seiten-x-1.btsp", "seiten-x-1.btsp.sig", "..", "", " ",
  ];
  const index = indexAusListe(liste);
  assert.equal(index.pakete.length, KERN_VERSIONEN_IM_INDEX);
  assert.deepEqual(index.pakete, [
    { kern: "0.10.0", folge: 1 },
    { kern: "0.9.430", folge: 1 },
    { kern: "0.9.421", folge: 3 },
  ]);
  assert.ok(MAX_FOLGE >= 1000);
});

pruefe("Paketname trägt Kern und Folge", () => {
  assert.equal(paketName("0.9.421", 2), "seiten-0.9.421-2.btsp");
});

// ── Ein echtes kleines Repository: Tag, Hotfix-Commits, Umgehungsversuche ──

function repoAnlegen() {
  const dir = mkdtempSync(join(tmpdir(), "seiten-repo-"));
  const git = (...args) =>
    execFileSync("git", ["-C", dir, ...args], { encoding: "utf8", stdio: "pipe" }).trim();
  git("init", "-q", "-b", "main");
  git("config", "user.email", "test@example.invalid");
  git("config", "user.name", "Test");
  git("config", "commit.gpgsign", "false");
  // Zeilenenden nicht umschreiben — sonst wichen die Bytes unter Windows ab.
  git("config", "core.autocrlf", "false");
  const schreibe = (pfad, text) => {
    mkdirSync(dirname(join(dir, pfad)), { recursive: true });
    writeFileSync(join(dir, pfad), text);
  };
  schreibe("src-tauri/tauri.conf.json", JSON.stringify({ version: "1.2.3" }));
  schreibe(
    "src-tauri/src/tablet/assets.rs",
    'seiten! {\n    Tablet => "tablet.html", ["__X__"];\n    Tl => "tl.html", [];\n}\n',
  );
  schreibe("src-tauri/src/lib.rs", "// kern\n");
  schreibe("src-tauri/assets/tablet.html", "<p>tablet __X__</p>\n");
  schreibe("src-tauri/assets/tl.html", "<p>tl</p>\n");
  schreibe("docs/a.md", "a\n");
  git("add", "-A");
  git("commit", "-q", "-m", "release");
  git("tag", "v1.2.3");
  const commit = (botschaft) => {
    git("add", "-A");
    git("commit", "-q", "-m", botschaft);
    return git("rev-parse", "HEAD");
  };
  return { dir, git, schreibe, commit };
}

pruefe("plan: reiner Seiten-Fix ergibt Kern des Tags und die richtige Folge", () => {
  const r = repoAnlegen();
  try {
    const tagCommit = r.git("rev-parse", "HEAD");
    assert.throws(
      () => plan({ repo: r.dir, tag: "v1.2.3", commit: tagCommit }),
      /Kein Commit seit/,
    );
    r.schreibe("docs/a.md", "b\n");
    const nurDoku = r.commit("doku");
    assert.throws(
      () => plan({ repo: r.dir, tag: "v1.2.3", commit: nurDoku }),
      /keine Geräte-Seite geändert/,
    );
    r.schreibe("src-tauri/assets/tablet.html", "<p>tablet __X__ <b>fix</b></p>\n");
    const fix = r.commit("fix");
    const p = plan({ repo: r.dir, tag: "v1.2.3", commit: fix });
    assert.equal(p.kern, "1.2.3");
    assert.equal(p.folge, 2);
    assert.equal(p.commit, fix);
    assert.deepEqual(p.namen, ["tablet.html", "tl.html"]);
    assert.deepEqual(p.seitenGeaendert, ["src-tauri/assets/tablet.html"]);

    // Der Bericht maskiert Seiteninhalt — er kann die Zusammenfassung nicht umformen.
    const text = bericht({ repo: r.dir, tag: "v1.2.3", commit: fix });
    assert.match(text, /Folge \*\*2\*\*/);
    assert.match(text, /&lt;b&gt;fix&lt;\/b&gt;/);
    assert.doesNotMatch(text, /<b>fix<\/b>/);

    // Nur vollständige Commit-Kennungen — kein Zweigname, keine Option.
    for (const falsch of ["HEAD", "main", "--output=x", fix.slice(0, 12), ""]) {
      assert.throws(
        () => plan({ repo: r.dir, tag: "v1.2.3", commit: falsch }),
        /Kein vollständiger Commit/,
        falsch,
      );
    }
    assert.throws(() => plan({ repo: r.dir, tag: "v9.9.9", commit: fix }));
  } finally {
    rmSync(r.dir, { recursive: true, force: true });
  }
});

pruefe("plan bricht ab: Kern geändert, Kern-Datei verschoben, Sonderzeichen im Namen", () => {
  const faelle = [
    ["Kern geändert", (r) => r.schreibe("src-tauri/src/lib.rs", "// anders\n"), /lib\.rs/],
    ["Cargo.lock neu", (r) => r.schreibe("Cargo.lock", "x\n"), /Cargo\.lock/],
    [
      "Kern-Datei nach docs verschoben",
      (r) => renameSync(join(r.dir, "src-tauri/src/lib.rs"), join(r.dir, "docs/lib.rs")),
      /src-tauri\/src\/lib\.rs/,
    ],
    ["Umlaut im Kern-Pfad", (r) => r.schreibe("src-tauri/src/ü.rs", "// x\n"), /ü\.rs/],
    [
      "Seitenliste geändert",
      (r) => r.schreibe("src-tauri/src/tablet/assets.rs", 'seiten! {\n    Tl => "tl.html", [];\n}\n'),
      /assets\.rs/,
    ],
  ];
  for (const [name, aendern, erwartet] of faelle) {
    const r = repoAnlegen();
    try {
      r.schreibe("src-tauri/assets/tl.html", "<p>tl fix</p>\n");
      aendern(r);
      const commit = r.commit(name);
      assert.throws(
        () => plan({ repo: r.dir, tag: "v1.2.3", commit }),
        (e) => /Kern-Pfade/.test(e.message) && erwartet.test(e.message),
        name,
      );
    } finally {
      rmSync(r.dir, { recursive: true, force: true });
    }
  }
});

pruefe("plan bricht ab: Seite ist ein Verweis; fremder Stand; Tag mit anderer Version", () => {
  const r = repoAnlegen();
  try {
    // Unter Windows legt git Verweise je nach Einstellung als gewöhnliche
    // Datei an; die Regel selbst prüft der Test weiter oben auf jedem System.
    if (process.platform !== "win32") {
      writeFileSync(join(r.dir, "geheim.txt"), "geheim\n");
      rmSync(join(r.dir, "src-tauri/assets/tl.html"));
      symlinkSync("../../geheim.txt", join(r.dir, "src-tauri/assets/tl.html"));
      const verweis = r.commit("verweis");
      assert.throws(
        () => plan({ repo: r.dir, tag: "v1.2.3", commit: verweis }),
        /tl\.html ist keine gewöhnliche Datei \(120000 blob\)/,
      );
    }

    // Ein Stand, der nicht vom Tag abstammt.
    r.git("checkout", "-q", "--orphan", "fremd");
    r.schreibe("src-tauri/assets/tl.html.neu", "x");
    const fremd = r.commit("fremd");
    assert.throws(
      () => plan({ repo: r.dir, tag: "v1.2.3", commit: fremd }),
      /stammt nicht vom Tag/,
    );
  } finally {
    rmSync(r.dir, { recursive: true, force: true });
  }
});

const skript = join(root, "scripts/build-seiten-paket.mjs");
const lauf = (...args) => execFileSync(process.execPath, [skript, ...args], { stdio: "pipe" });

pruefe("Kommandozeile: derselbe Stand ergibt dieselben Bytes, gelesen aus git statt von der Platte", () => {
  const r = repoAnlegen();
  const aus = mkdtempSync(join(tmpdir(), "seiten-paket-"));
  try {
    r.schreibe("src-tauri/assets/tablet.html", "<p>tablet __X__ fix</p>\n");
    const fix = r.commit("fix");
    // Die Platte lügt: Im Arbeitsverzeichnis steht danach etwas anderes.
    r.schreibe("src-tauri/assets/tablet.html", "<p>NICHT COMMITTET</p>\n");

    const [a, b] = [join(aus, "a.btsp"), join(aus, "b.btsp")];
    for (const out of [a, b]) {
      lauf("nutzlast", "--repo", r.dir, "--tag", "v1.2.3", "--commit", fix, "--out", out);
    }
    assert.deepEqual(readFileSync(a), readFileSync(b), "Bau ist wiederholbar");
    const inhalt = JSON.parse(gunzipSync(readFileSync(a)).toString("utf8"));
    assert.equal(inhalt.kern, "1.2.3");
    assert.equal(inhalt.folge, 1);
    assert.equal(inhalt.commit, fix);
    assert.equal(inhalt.seiten["tablet.html"], "<p>tablet __X__ fix</p>\n");
    assert.deepEqual(Object.keys(inhalt.seiten).sort(), ["tablet.html", "tl.html"]);

    const plan = lauf("plan", "--repo", r.dir, "--tag", "v1.2.3", "--commit", fix).toString();
    assert.match(plan, /^kern=1\.2\.3\nfolge=1\ncommit=[0-9a-f]{40}\ntag_commit=[0-9a-f]{40}\npaket=seiten-1\.2\.3-1\.btsp\n$/);

    // nutzlast prüft selbst: Ein Stand mit Kern-Änderung wird nie gebaut.
    r.schreibe("src-tauri/src/lib.rs", "// anders\n");
    const kern = r.commit("kern");
    assert.throws(() =>
      lauf("nutzlast", "--repo", r.dir, "--tag", "v1.2.3", "--commit", kern, "--out", join(aus, "c.btsp")),
    );
    assert.throws(() => readFileSync(join(aus, "c.btsp")));

    const liste = join(aus, "liste.txt");
    writeFileSync(liste, "seiten-1.2.3-1.btsp\nseiten-1.2.3-1.btsp.sig\nlatest.json\n");
    const neu = join(aus, "seiten.json");
    lauf("index", "--liste", liste, "--out", neu);
    assert.equal(readFileSync(neu, "utf8"), '{"format":1,"pakete":[{"kern":"1.2.3","folge":1}]}\n');
  } finally {
    rmSync(r.dir, { recursive: true, force: true });
    rmSync(aus, { recursive: true, force: true });
  }
});

pruefe("Kommandozeile für den Rust-Test: Paket aus den echten Seiten, unter dem Deckel der App", () => {
  const aus = mkdtempSync(join(tmpdir(), "seiten-paket-"));
  try {
    const paket = join(aus, "p.btsp");
    lauf("nutzlast", "--aus-dateien", root, "--kern", "0.9.421", "--folge", "1", "--commit", "abc", "--out", paket);
    const inhalt = JSON.parse(gunzipSync(readFileSync(paket)).toString("utf8"));
    assert.equal(
      inhalt.seiten["tablet.html"],
      readFileSync(join(root, "src-tauri/assets/tablet.html"), "utf8"),
    );
    assert.deepEqual(Object.keys(inhalt.seiten).sort(), [...seitenNamen].sort());
    assert.ok(readFileSync(paket).length < 8 * 1024 * 1024, "unter dem 8-MB-Deckel der App");
  } finally {
    rmSync(aus, { recursive: true, force: true });
  }
});

pruefe("Workflow: nur von Hand, Zweig nur als Daten, Secrets erst nach der Freigabe", () => {
  const wf = readFileSync(join(root, ".github/workflows/seiten-paket.yml"), "utf8");
  const ohneKommentare = wf.split("\n").filter((z) => !/^\s*#/.test(z)).join("\n");
  const ausloeser = ohneKommentare.slice(ohneKommentare.indexOf("\non:"), ohneKommentare.indexOf("\npermissions:"));
  assert.match(ausloeser, /workflow_dispatch:/);
  assert.doesNotMatch(ausloeser, /\bpush\b|pull_request|schedule|workflow_run/);

  const jobs = ohneKommentare.slice(ohneKommentare.indexOf("\njobs:"));
  const grenze = jobs.indexOf("\n  signieren:");
  const [davor, signieren] = [jobs.slice(0, grenze), jobs.slice(grenze)];
  assert.match(signieren, /environment: seiten-hotfix/);
  assert.doesNotMatch(davor, /secrets\./, "vor der Freigabe darf kein Secret gelesen werden");
  assert.match(signieren, /TAURI_SIGNING_PRIVATE_KEY/);
  for (const job of [davor, signieren]) {
    assert.match(job, /if: github\.ref == 'refs\/heads\/main'/);
    // Der Hotfix-Zweig wird nie ausgecheckt: kein `ref:` auf ihn, nur der
    // nackte Klon, den das Skript aus `main` über git liest.
    assert.doesNotMatch(job, /ref: .*hotfix/);
    assert.match(job, /--repo zweig\.git/);
    assert.match(job, /persist-credentials: false/);
    // Actions auf einen festen Stand genagelt.
    for (const m of job.matchAll(/uses: (\S+)/g)) assert.match(m[1], /@[0-9a-f]{40}$/, m[1]);
  }
  // Kein `${{ … }}` in einem run-Block außer in env — nichts wird in die
  // Shell hineinkopiert.
  for (const block of jobs.matchAll(/run: \|\n((?:\s{10,}.*\n?|\n)+)/g)) {
    assert.doesNotMatch(block[1], /\$\{\{/, block[1].slice(0, 80));
  }
  assert.doesNotMatch(signieren, /cache: npm/, "kein Cache im Job mit dem Schlüssel");
  // Signiert wird nur, was dem Probebau der Freigabe gleicht.
  assert.ok(signieren.indexOf("sha256sum --check") < signieren.indexOf("signer sign"));
  assert.ok(signieren.indexOf("sha256sum --check") > 0);
  // Erst die Nutzlast, dann der Index.
  assert.ok(signieren.indexOf("Paket hochladen") > 0);
  assert.ok(signieren.indexOf("Paket hochladen") < signieren.indexOf("Index hochladen"));
  assert.doesNotMatch(signieren, /--delete/);
});

console.log(`\n${anzahl} Prüfungen bestanden.`);
