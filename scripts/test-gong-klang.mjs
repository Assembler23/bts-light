// Testet den Gong-Klang (src/io/gongKlang.mjs) mit einem nachgebauten
// AudioContext und hält die Inline-Kopie in assets/ansage.html wortgleich —
// so klingt der Gong auf der Ansage-Seite wie am Turnier-PC.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { gongPlanen } from "../src/io/gongKlang.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

/** AudioContext-Attrappe: zeichnet Oszillatoren auf. */
function attrappe() {
  const oszillatoren = [];
  const param = () => ({ setValueAtTime() {}, exponentialRampToValueAtTime() {}, value: 0 });
  return {
    oszillatoren,
    currentTime: 10,
    destination: {},
    createGain: () => ({ gain: param(), connect() {} }),
    createOscillator() {
      const o = { type: "", frequency: param(), connect() {}, start(t) { o.startT = t; }, stop(t) { o.stopT = t; } };
      oszillatoren.push(o);
      return o;
    },
  };
}

let ctx = attrappe();
let g = gongPlanen(ctx, "match");
ok("Spielaufruf: zwei Sinus-Töne", ctx.oszillatoren.map((o) => [o.type, o.frequency.value]), [["sine", 880], ["sine", 587.33]]);
ok("Spielaufruf: letzter ist der tiefe Ton", g.letzter === ctx.oszillatoren[1], true);
ok("Spielaufruf: Ende 1,2 s nach Start", Math.round((g.endeSec - 10) * 100) / 100, 1.2);

ctx = attrappe();
g = gongPlanen(ctx, "info");
ok("Info: drei Dreieck-Töne aufsteigend", ctx.oszillatoren.map((o) => [o.type, o.frequency.value]), [["triangle", 523.25], ["triangle", 659.25], ["triangle", 783.99]]);
ok("Info: Ende am Stopp des dritten Tons", Math.round((g.endeSec - 10) * 100) / 100, 0.68);
ok("Info: letzter ist der dritte Ton", g.letzter === ctx.oszillatoren[2], true);

{
  const hier = dirname(fileURLToPath(import.meta.url));
  const modul = readFileSync(join(hier, "..", "src", "io", "gongKlang.mjs"), "utf8");
  const html = readFileSync(join(hier, "..", "src-tauri", "assets", "ansage.html"), "utf8");
  const erwartet = modul.split("\n\n").slice(1).join("\n\n").replace(/^export /gm, "").trim();
  const m = html.match(/\/\* <gongKlang\.mjs> \*\/\n([\s\S]*?)\n\/\* <\/gongKlang\.mjs> \*\//);
  ok("ansage.html trägt die Kopie", !!m, true);
  ok("Kopie gleicht dem Modul", m && m[1].trim() === erwartet, true);
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("Alle Gong-Tests grün.");
