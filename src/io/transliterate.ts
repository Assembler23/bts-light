// Regelbasierte Umschrift fremdsprachiger Namen in eine **deutsche Lautschrift-
// Näherung** für die TTS-Ansage. Greift dort, wo das mitgelieferte Wörterbuch
// (`nameOverrideBase.ts`) und die Nutzer-Tabelle keinen Treffer haben — so
// werden auch NICHT gelistete chinesische/vietnamesische Namen besser gelesen.
//
// EHRLICHE GRENZEN:
// - **Konsonanten** sind die großen Fehlerquellen einer deutschen Stimme und
//   werden zuverlässig korrigiert (zh→dsch, x→sch, tr→tsch, ph→f, kh→ch, …).
// - **Vokale/Endungen/Töne** sind dialektabhängig (v. a. Vietnamesisch) und
//   bleiben eine Standard-Näherung — nicht jede Feinheit ist abbildbar.
// - Zielschreibweise = **deutsche Leseregeln** (z. B. „Sch" statt „S", weil ein
//   anlautendes deutsches „S" stimmhaft [z] gelesen würde; „i" statt „y").
//
// Sicherheits-Prinzip: Die Umschrift wird NUR auf einen Namen angewendet, der
// über einen **markanten chinesischen bzw. vietnamesischen Nachnamen** als
// solcher erkannt wurde (`detectNameLang`). Deutsche/andere Namen bleiben
// unangetastet — deshalb sind innerhalb eines erkannten Namens auch aggressive
// Regeln (v→w, d→j) gefahrlos.

import { NAME_LANG_BASE } from "./nameLangBase";

// Herkunftssprachen für die Aussprache. cn/vn haben zusätzlich eine deutsche
// Umschrift-Engine (Web-Speech-Fallback); die übrigen werden nur im Azure-
// `<lang>`-Pfad nativ gesprochen (keine deutsche Umschrift).
export type NameLang = "cn" | "vn" | "es" | "fr" | "pl" | "tr" | "ms" | "in";

// Diakritika/Sonderzeichen falten (wie announcer.normalizeName). đ wird hier
// bewusst NICHT zu d gefaltet (die Onset-Logik braucht die Unterscheidung).
function fold(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[ıİ]/g, "i")
    .trim()
    .toLowerCase();
}

// ── Sprach-Erkennung: markante Trigger-Nachnamen ────────────────────────────
// Bewusst nur EINDEUTIGE Nachnamen (keine kurzen, mehrdeutigen wie „Le", „Do",
// „Ma", „Li", „Lin", „Han", „Tan"). Ein Name gilt als CN/VN, wenn IRGENDEIN
// Token ein Trigger ist (fängt auch zusammengesetzte Nachnamen).
const CN_TRIGGERS = new Set([
  "zhang", "wang", "chen", "liu", "huang", "zhao", "zhou", "xu", "zhu", "wu",
  "zheng", "jiang", "xie", "cheng", "shen", "guo", "luo", "wong", "cheung",
  "qin", "qiu", "cai", "cao", "feng", "peng", "ruan", "guan", "zhong",
  "xiong", "kong", "deng", "yang",
]);
const VN_TRIGGERS = new Set([
  "nguyen", "tran", "pham", "phan", "truong", "huynh", "hoang", "vu", "dang",
  "bui", "duong", "phung", "trinh", "dao", "dinh", "vo", "ngo", "doan",
  "luong",
  // „dam" bewusst NICHT als Trigger (kollidiert mit niederländisch „van Dam");
  // das seltene VN Đàm ggf. über das Nutzer-Wörterbuch abdecken.
]);

// Erkennt die Herkunftssprache eines Namens. Reihenfolge = Konfidenz:
// 1) markante CN/VN-Nachnamen (starkes Signal; VN vor CN, Silben überlappen);
// 2) kuratierte Namenslisten `NAME_LANG_BASE` (es/fr/pl/tr/ms/in) — erst der
//    ganze Name, dann Tokens, und NUR wenn EINDEUTIG genau eine Sprache.
// Bei nichts/mehrdeutig → `null` (deutscher Default statt souverän falsch).
export function detectNameLang(tokens: string[]): NameLang | null {
  const folded = tokens.map((t) => fold(t).replace(/[đĐ]/g, "d"));
  if (folded.some((k) => VN_TRIGGERS.has(k))) return "vn";
  if (folded.some((k) => CN_TRIGGERS.has(k))) return "cn";
  // Ganzer Name zuerst (Vollname-Einträge), dann einzelne Tokens.
  const full = NAME_LANG_BASE.get(folded.join(" "));
  if (full) return full;
  const hits = new Set<NameLang>();
  for (const k of folded) {
    const l = NAME_LANG_BASE.get(k);
    if (l) hits.add(l);
  }
  return hits.size === 1 ? [...hits][0] : null;
}

function cap(w: string): string {
  return w ? w.charAt(0).toUpperCase() + w.slice(1) : w;
}

// ── Pinyin (Mandarin) → Deutsch ─────────────────────────────────────────────
const PINYIN_INITIALS: [string, string][] = [
  ["zh", "dsch"], ["ch", "tsch"], ["sh", "sch"],
  ["x", "sch"], ["q", "tsch"], ["j", "dsch"],
  ["c", "ts"], ["z", "ds"], ["r", "r"],
  ["b", "b"], ["p", "p"], ["m", "m"], ["f", "f"],
  ["d", "d"], ["t", "t"], ["n", "n"], ["l", "l"],
  ["g", "g"], ["k", "k"], ["h", "h"], ["s", "ss"],
  ["w", "u"], ["y", "j"],
];
const PINYIN_FINALS: [string, string][] = [
  ["iang", "jang"], ["uang", "uang"], ["iong", "jong"], ["ueng", "ueng"],
  ["uai", "uai"], ["iao", "jao"], ["ian", "jän"], ["uan", "uan"],
  ["ang", "ang"], ["eng", "eng"], ["ing", "ing"], ["ong", "ong"],
  ["iu", "jou"], ["ui", "uei"], ["un", "un"], ["ua", "ua"], ["uo", "uo"],
  ["ie", "jeh"], ["ai", "ai"], ["ei", "ei"], ["ao", "ao"], ["ou", "ou"],
  ["an", "an"], ["en", "en"], ["in", "in"], ["er", "er"], ["ia", "ja"],
  ["ve", "üe"], ["ue", "üe"],
  ["a", "a"], ["o", "o"], ["e", "e"], ["i", "i"], ["u", "u"], ["v", "ü"],
];
const FINAL_KEYS = PINYIN_FINALS.map(([f]) => f).sort((a, b) => b.length - a.length);
const INITIAL_KEYS = PINYIN_INITIALS.map(([i]) => i).sort((a, b) => b.length - a.length);
const APICAL_INITIALS = new Set(["zh", "ch", "sh", "z", "c", "s", "r"]);
const mapInitial = (k: string) => PINYIN_INITIALS.find(([x]) => x === k)?.[1] ?? k;
const mapFinal = (k: string) => PINYIN_FINALS.find(([x]) => x === k)?.[1] ?? k;

// Zerlegt einen (ggf. mehrsilbig zusammengeschriebenen) Pinyin-Token in Silben
// und setzt jede als deutsche Näherung zusammen. „Zhixin" → „Dschi-Schin",
// „Yinsong" → „In-Ssong". Greedy: längstes Initial + längstes gültiges Finale.
export function pinyinToGerman(token: string): string {
  const s = fold(token).replace(/[đĐ]/g, "d");
  if (!s) return token;
  const out: string[] = [];
  let i = 0;
  let guard = 0;
  while (i < s.length && guard++ < 40) {
    let init = "";
    for (const k of INITIAL_KEYS) {
      if (s.startsWith(k, i)) { init = k; break; }
    }
    const j = i + init.length;
    let fin = "";
    for (const k of FINAL_KEYS) {
      if (s.startsWith(k, j)) { fin = k; break; }
    }
    if (!fin) { out.push(s.slice(i)); break; }
    let initOut = mapInitial(init);
    let finOut = mapFinal(fin);
    // Anlaut y/w sind nur Schreibhilfen: yi→i, yin→in, ying→ing, yu→ü;
    // wu→u, wang→uang.
    if (init === "y") {
      if (fin.startsWith("i")) initOut = "";
      else if (fin === "u" || fin === "v") { initOut = ""; finOut = "ü"; }
      else { initOut = "j"; }
    }
    if (init === "w") {
      initOut = fin.startsWith("u") ? "" : "u";
    }
    // Nach j/q/x wird „u" als [y] (ü) gesprochen: xu→schü, ju→dschü, jun→dschün.
    if (init === "j" || init === "q" || init === "x") {
      if (fin === "u") finOut = "ü";
      else if (fin === "un") finOut = "ün";
      else if (fin === "uan") finOut = "üan";
      // Medialer Gleitlaut steckt schon im palatalen Anlaut → kein Doppel-„j":
      // jian→Dschän (nicht Dschjän), jiang→Dschang.
      else if (fin === "ian") finOut = "än";
      else if (fin === "iang") finOut = "ang";
    }
    // Apikales i („zhi/chi/shi/zi/ci/si/ri") → schlichtes „i".
    if (APICAL_INITIALS.has(init) && fin === "i") finOut = "i";
    out.push(initOut + finOut);
    i = j + fin.length;
  }
  return cap(out.join("-"));
}

// ── Vietnamesisch → Deutsch (konsonant-fokussiert, südvietnamesisch-nah) ─────
// Onsets (längste zuerst). „d" (ohne Strich) ≈ [j] (südl.), „đ" = echtes [d];
// „v" ≈ [j]/[v] → „w" (deutsches w = [v]); beides nur innerhalb erkannter
// VN-Namen, daher kollisionsfrei.
const VN_ONSETS: [string, string][] = [
  ["ngh", "ng"], ["ng", "ng"], ["nh", "nj"], ["tr", "tsch"], ["th", "t"],
  ["ph", "f"], ["ch", "tsch"], ["kh", "ch"], ["gh", "g"], ["gi", "j"],
  ["qu", "ku"], ["x", "s"], ["v", "w"], ["r", "r"],
];
// „d" bewusst NICHT umgeschrieben: ASCII verschluckt das đ (Đào/Đặng = hartes
// [d]); ein generelles d→j würde diese häufigen Namen zerstören. Weiche d-Fälle
// (z. B. Dương→Juong) deckt das Basis-Wörterbuch ab.
const VN_CODAS: [string, string][] = [
  ["nh", "n"], ["ng", "ng"], ["ch", "k"], ["c", "k"],
];

export function vietnameseToGerman(token: string): string {
  const t = token.trim();
  if (!t) return token;
  // đ am Anfang = echtes [d] (Đặng, Đỗ, Đào, Đinh); sonst ist „d" das weiche [j].
  const hardInitialD = /^[đĐ]/.test(t);
  const s = fold(t).replace(/[đĐ]/g, "d");
  if (!s) return token;
  let onset = "";
  let rest = s;
  let matched = false;
  for (const [k, v] of VN_ONSETS) {
    if (s.startsWith(k)) {
      onset = k === "d" && hardInitialD ? "d" : v;
      rest = s.slice(k.length);
      matched = true;
      break;
    }
  }
  if (!matched) {
    onset = s.charAt(0);
    rest = s.slice(1);
  }
  for (const [k, v] of VN_CODAS) {
    if (rest.endsWith(k)) {
      rest = rest.slice(0, rest.length - k.length) + v;
      break;
    }
  }
  return cap(onset + rest);
}

// Setzt die passende Engine je erkannter Sprache auf einen Token an. Nur cn/vn
// haben eine deutsche Umschrift; für es/fr/pl/tr/ms/in bleibt der Token roh
// (deren native Aussprache läuft über den Azure-`<lang>`-Pfad, nicht über die
// deutsche Web-Speech-Stimme).
export function transliterateToken(token: string, lang: NameLang): string {
  if (lang === "cn") return pinyinToGerman(token);
  if (lang === "vn") return vietnameseToGerman(token);
  return token;
}
