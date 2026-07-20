// Gesprochene Feld-Ansage beim Aufruf eines Spiels auf einen Court.
//
// Browser-nativ: Web Audio API für den Gong + SpeechSynthesisUtterance fürs
// TTS. Keine externen Dienste, keine Daten verlassen das Gerät. Portiert aus
// badhub-tournament (src/io/announcer.ts) und an das bts-light-Ansageformat
// angepasst: Gong → Feld → Disziplin → Paarung → Feld.
//
// WebView2 (Windows) startet den AudioContext oft erst nach einer
// Nutzergeste — der Test-Knopf in den Einstellungen und ein einmaliger
// globaler Klick-Listener schalten das Audio für die Session frei.

import type { AnnounceLanguageMode, Discipline, NameOverride } from "../types";
import { reportAzureFallback } from "../state/azureStatus";
import { BASE_NAME_OVERRIDES } from "./nameOverrideBase";
import { detectNameLang, transliterateToken } from "./transliterate";
import type { NameLang } from "./transliterate";

export type AnnounceLang = "de" | "en";

export interface AnnounceMatchInput {
  /** Court-Label wie BTP es liefert, z. B. "1", "Feld 2", "Center Court". */
  courtLabel: string;
  /** Disziplin des Matches. */
  discipline: Discipline;
  /** Spieler-Namen Team A (1 bei Einzel, 2 bei Doppel/Mixed). */
  teamANames: string[];
  /** Spieler-Namen Team B. */
  teamBNames: string[];
  /** Reine BTP-Runde (z. B. "VF", "Finale"). Wird AB Viertelfinale vor der
   *  Paarung mitangesagt; frühere Runden/Gruppen werden nicht angesagt. */
  roundName?: string;
  /** Klassen-Kürzel („A", „B", „U15" …) — wird direkt hinter der Disziplin
   *  angesagt („Herreneinzel A"). Leer/fehlend = keine Klasse. Gruppen-
   *  namen gehören hier NICHT hinein (Nutzer-Vorgabe: nie „Gruppe 3"). */
  className?: string;
}

export interface AnnounceOptions {
  /** Sprech-Geschwindigkeit; fällt sonst auf DEFAULT_SPEECH_RATE zurück. */
  rate?: number;
  /** Voice-URI der gewünschten Stimme; sonst OS-Default für die Sprache. */
  voiceURI?: string;
  /** Gong vor der Ansage abspielen? Default true. */
  gong?: boolean;
  /** Phonetische Aussprache-Korrekturen des Nutzers (Vorrang vor der Basis). */
  nameOverrides?: NameOverride[];
  /** Aussprache-Korrekturen anwenden? Default true (Basis + Nutzer-Einträge). */
  nameOverridesEnabled?: boolean;
  /** Hochwertige Azure-Ansage: ganze Ansage als SSML synthetisieren + abspielen.
   *  `synthesize(ssml)` liefert MP3 als Base64; wirft bei Fehler → Fallback auf
   *  Web Speech. Fehlt das Feld, läuft alles wie bisher (Web Speech). */
  azure?: { voice: string; synthesize: (ssml: string) => Promise<string> };
}

// Kleine Atempause zwischen Gong-Ende und Sprachbeginn (ms) – klingt
// natürlicher und puffert Restjitter der Audio-/OS-Uhr.
const GONG_BREATH_MS = 150;

// Löst auf, sobald der Gong WIRKLICH ausgeklungen ist: am echten Audio-Ende
// (`onended` des zuletzt stoppenden Oszillators) statt auf einer festen
// Wall-Clock-Uhr. Startet der AudioContext in WebView2 verzögert, verschiebt
// sich das Ende real mit — die Sprache setzt dann nicht mehr in den
// Gong-Nachklang ein (Tilo-Befund 19.07.: „Gong überlappt das erste Wort").
// Fallback-Timer etwas nach dem geplanten Ende, falls `onended` in WebView2
// ausnahmsweise ausbleibt (die Ansage-Queue darf nie hängen).
function gongFinished(
  ctx: AudioContext,
  lastOsc: OscillatorNode,
  scheduledEndSec: number,
): Promise<void> {
  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      setTimeout(resolve, GONG_BREATH_MS);
    };
    lastOsc.onended = finish;
    const fallbackMs =
      Math.max(0, (scheduledEndSec - ctx.currentTime) * 1000) + 250;
    setTimeout(finish, fallbackMs);
  });
}

// Synthesizer-Gong über Web Audio. Zwei kurze Sinus-Töne (hoch → tiefer) mit
// kleinem Decay, ähnlich einem Hotel-Gong. Liefert eine Promise, die
// resolved, wenn der Gong durchgespielt ist — damit die Sprache erst danach
// startet.
async function playGong(
  ctx: AudioContext,
  kind: "match" | "info" = "match",
): Promise<void> {
  const now = ctx.currentTime;

  // Info/Freitext: KLAR anderer Klang als der Spielaufruf – drei helle,
  // perlende Töne (C-Dur-Dreiklang aufsteigend, Triangle-Wellenform, kurz
  // gestoßen). Unterscheidet sich in Tonzahl (3 statt 2), Klangfarbe
  // (Triangle statt Sinus) und Bewegung deutlich vom tiefen 2-Ton-Spielaufruf.
  if (kind === "info") {
    const notes = [523.25, 659.25, 783.99]; // C5 – E5 – G5
    const step = 0.15;
    let lastOsc: OscillatorNode | null = null;
    let lastStop = now;
    for (let i = 0; i < notes.length; i++) {
      const t = now + i * step;
      const g = ctx.createGain();
      g.gain.setValueAtTime(0.0001, t);
      g.gain.exponentialRampToValueAtTime(0.32, t + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, t + 0.32);
      g.connect(ctx.destination);
      const o = ctx.createOscillator();
      o.type = "triangle";
      o.frequency.value = notes[i];
      o.connect(g);
      o.start(t);
      const stopAt = t + 0.38;
      o.stop(stopAt);
      lastOsc = o;
      lastStop = stopAt;
    }
    // Auflösen, wenn der dritte Ton ausgeklungen ist (~0,85 s ab now).
    return lastOsc ? gongFinished(ctx, lastOsc, lastStop) : Promise.resolve();
  }

  // Spielaufruf = tiefer, zweitöniger ABSTEIGENDER Sinus-Gong (A5 → D5),
  // wie ein Hotel-Gong.
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(0.4, now + 0.05);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + 1.2);
  gain.connect(ctx.destination);

  const o1 = ctx.createOscillator();
  o1.type = "sine";
  o1.frequency.value = 880;
  o1.connect(gain);
  o1.start(now);
  o1.stop(now + 0.6);

  const o2 = ctx.createOscillator();
  o2.type = "sine";
  o2.frequency.value = 587.33;
  o2.connect(gain);
  o2.start(now + 0.18);
  o2.stop(now + 1.1);

  // Erst auflösen, wenn die Gain-Hülle ausgeklungen ist (~1,2 s) – am echten
  // Ende des zuletzt stoppenden Oszillators (o2), damit die Sprachausgabe
  // nicht in den Gong-Nachklang einsetzt.
  return gongFinished(ctx, o2, now + 1.2);
}

// Reusable AudioContext — Browser-Limit von 1–6 contexts pro Tab; einer reicht.
let cachedCtx: AudioContext | null = null;
function getAudioContext(): AudioContext {
  if (cachedCtx == null) {
    cachedCtx = new (
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext })
        .webkitAudioContext
    )();
  }
  return cachedCtx;
}

// Schaltet das Audio frei (WebView2/Browser starten den AudioContext nur
// nach einer Nutzergeste). Bei jedem echten Klick im Fenster aufrufbar.
export function unlockAudio(): void {
  try {
    const ctx = getAudioContext();
    if (ctx.state === "suspended") void ctx.resume();
  } catch {
    // AudioContext nicht verfügbar – ignorieren.
  }
}

// Spielt ein Base64-MP3 (von Azure TTS) über den AudioContext und löst auf,
// wenn es fertig ist. Wirft bei ungültigem Audio (→ Aufrufer-Fallback).
// Aktuell laufende Azure-Audioquelle — damit cancelAnnouncements() sie stoppen
// kann (Web-Audio-Pendant zu speechSynthesis.cancel()).
let activeAzureSrc: AudioBufferSourceNode | null = null;

async function playMp3Base64(b64: string): Promise<void> {
  const ctx = getAudioContext();
  if (ctx.state === "suspended") {
    try {
      await ctx.resume();
    } catch {
      /* ignore */
    }
  }
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const buf = await ctx.decodeAudioData(bytes.buffer);
  await new Promise<void>((resolve) => {
    const src = ctx.createBufferSource();
    activeAzureSrc = src;
    src.buffer = buf;
    src.connect(ctx.destination);
    src.onended = () => {
      if (activeAzureSrc === src) activeAzureSrc = null;
      resolve();
    };
    src.start();
  });
}

// ─── Globale Ansage-Warteschlange ────────────────────────────────────────
// ALLE Ansagen (Feld-Auto-Ansage, manuelle Ansage, Vorbereitung) laufen
// strikt nacheinander durch DIESE eine Kette — so kann nie ein Gong starten,
// während eine vorige Ansage noch spricht (Feld-Bug 2026-06-14). Eine
// „Generation" entwertet noch wartende Aufgaben, wenn abgebrochen wird.
let announceQueue: Promise<void> = Promise.resolve();
let announceGen = 0;

function enqueueAnnouncement(task: () => Promise<void>): Promise<void> {
  const gen = announceGen;
  const run = announceQueue.then(() =>
    gen === announceGen ? task() : undefined,
  );
  // Kette auch nach einem Fehler weiterlaufen lassen.
  announceQueue = run.then(
    () => {},
    () => {},
  );
  return run;
}

// Spielt (optional) den Gong und wartet, bis er ausgeklungen ist. `kind`
// unterscheidet Spielaufruf (Standard) von Info/Freitext (anderer Klang).
async function maybeGong(
  gong: boolean | undefined,
  kind: "match" | "info" = "match",
): Promise<void> {
  if (gong === false) return;
  try {
    const ctx = getAudioContext();
    if (ctx.state === "suspended") {
      try {
        await ctx.resume();
      } catch {
        // ignore
      }
    }
    await playGong(ctx, kind);
  } catch {
    // Gong fehlgeschlagen → trotzdem sprechen.
  }
}

// Sprech-Geschwindigkeit. Browser-Default ist 1.0 (oft zu schnell für eine
// Hallen-Durchsage). 0.8 ist ein guter Mittelweg.
export const DEFAULT_SPEECH_RATE = 0.8;

function clampRate(rate: number | undefined): number {
  if (rate == null || !Number.isFinite(rate)) return DEFAULT_SPEECH_RATE;
  // Unter 0.5 wird es unverständlich tieftönend, über 1.5 hetzt es.
  if (rate < 0.5) return 0.5;
  if (rate > 1.5) return 1.5;
  return rate;
}

// Spricht EIN Segment und löst auf, wenn es FERTIG gesprochen ist (`onend`).
// Wichtig fürs strikt sequenzielle Abspielen: nur so kann die nächste Ansage
// (und ihr Gong) erst nach dem Sprechende der vorigen starten. Fallback-Timeout,
// falls `onend` in WebView2 mal nicht feuert — dann hängt die Queue nicht.
function speakSegment(
  text: string,
  lang: AnnounceLang,
  rate: number,
  voiceURI?: string,
): Promise<void> {
  return new Promise((resolve) => {
    if (typeof window.speechSynthesis === "undefined" || !text.trim()) {
      resolve();
      return;
    }
    const u = new SpeechSynthesisUtterance(text.trim());
    u.lang = lang === "de" ? "de-DE" : "en-US";
    u.rate = clampRate(rate);
    u.pitch = 1;
    u.volume = 1;
    if (voiceURI) {
      const voices = window.speechSynthesis.getVoices();
      const match = voices.find((v) => v.voiceURI === voiceURI);
      if (match) u.voice = match;
    }
    let done = false;
    let timer = 0;
    const finish = () => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      resolve();
    };
    u.onend = finish;
    u.onerror = finish;
    // Großzügige Obergrenze; `onend` feuert im Normalfall vorher.
    timer = window.setTimeout(finish, 2000 + text.trim().length * 140);
    window.speechSynthesis.speak(u);
  });
}

// Spricht mehrere Segmente nacheinander, jeweils bis zum Sprechende.
async function speakSegments(
  segments: string[],
  lang: AnnounceLang,
  rate: number,
  voiceURI?: string,
): Promise<void> {
  for (const seg of segments) {
    await speakSegment(seg, lang, rate, voiceURI);
  }
}

// Browser-TTS spricht "Feld 1" gern als "Feld erste" (Ordinal-Heuristik).
// Kleine Zahlen daher als Wort ausschreiben.
const NUMBER_WORDS_DE = [
  "",
  "eins",
  "zwei",
  "drei",
  "vier",
  "fünf",
  "sechs",
  "sieben",
  "acht",
  "neun",
  "zehn",
  "elf",
  "zwölf",
  "dreizehn",
  "vierzehn",
  "fünfzehn",
  "sechzehn",
  "siebzehn",
  "achtzehn",
  "neunzehn",
  "zwanzig",
];
const NUMBER_WORDS_EN = [
  "",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
  "twelve",
  "thirteen",
  "fourteen",
  "fifteen",
  "sixteen",
  "seventeen",
  "eighteen",
  "nineteen",
  "twenty",
];

function numberWord(n: number, lang: AnnounceLang): string {
  if (!Number.isFinite(n) || n < 1) return String(n);
  const idx = Math.floor(n);
  const list = lang === "de" ? NUMBER_WORDS_DE : NUMBER_WORDS_EN;
  return idx < list.length ? list[idx] : String(idx);
}

// BTP-Court-Labels sind frei benennbar ("1", "Feld 2", "Center Court").
// Endet das Label auf einer Zahl, sprechen wir "Feld <Zahlwort>"; sonst das
// Label wörtlich (kein "Feld Center Court").
function resolveCourtPhrase(label: string, lang: AnnounceLang): string {
  const trimmed = label.trim();
  const m = trimmed.match(/(\d{1,3})\s*$/);
  if (m) {
    const word = numberWord(Number(m[1]), lang);
    return lang === "de" ? `Feld ${word}` : `Court ${word}`;
  }
  return trimmed;
}

function disciplineWord(d: Discipline, lang: AnnounceLang): string {
  const words: Record<AnnounceLang, Record<Discipline, string>> = {
    de: {
      mens_singles: "Herreneinzel",
      womens_singles: "Dameneinzel",
      mens_doubles: "Herrendoppel",
      womens_doubles: "Damendoppel",
      mixed: "Mixed",
      unknown: "",
    },
    en: {
      mens_singles: "Men's Singles",
      womens_singles: "Women's Singles",
      mens_doubles: "Men's Doubles",
      womens_doubles: "Women's Doubles",
      mixed: "Mixed",
      unknown: "",
    },
  };
  return words[lang][d] ?? "";
}

// Disziplin + Klassen-Kürzel („Herreneinzel A") — die Klasse kommt direkt
// hinter die Disziplin (Turnier-Wunsch 17.07.2026). Ohne erkannte Disziplin
// wird auch keine Klasse angesagt (ein nacktes „A." wäre unverständlich).
// Exportiert, damit die Slave-Spielübersicht (Cluster C Stufe 2) dieselbe
// Beschriftung anzeigt, die auch angesagt wird.
export function disciplineWithClass(
  d: Discipline,
  className: string | undefined,
  lang: AnnounceLang,
): string {
  const disc = disciplineWord(d, lang);
  const cls = (className || "").trim();
  return disc && cls ? `${disc} ${cls}` : disc;
}

// Schlüssel-Normalisierung fürs Matching: Diakritika UND fremde Sonderbuchstaben
// falten, damit z. B. „Nguyên"/„Nguyen", „Yıldız"/„Yildiz", „García"/„Garcia"
// alle denselben Wörterbuch-Eintrag treffen. NFD + Entfernen kombinierender
// Marken deckt ê,é,ä,ñ,ş,ç,ğ … ab; eigenständige Buchstaben (ı,ø,ł,đ) explizit.
function normalizeName(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[ıİ]/g, "i")
    .replace(/[øØ]/g, "o")
    .replace(/[łŁ]/g, "l")
    .replace(/[đĐ]/g, "d")
    .trim()
    .toLowerCase();
}

// Baut die Lookup-Map (normalisierter Schlüssel → gesprochene Form): zuerst das
// mitgelieferte Basis-Wörterbuch, dann die Nutzer-Einträge — Letztere
// ÜBERSCHREIBEN die Basis bei gleichem Schlüssel (Nutzer hat Vorrang). Ist die
// Korrektur ausgeschaltet (`enabled === false`), bleibt die Map leer → Namen
// werden 1:1 vorgelesen.
// Geteiltes Wörterbuch (crowd-sourced, von badhub geladen). Liegt zwischen
// Basis-Wörterbuch (niedrigste Priorität) und Nutzer-Tabelle (höchste). Wird
// beim Start/periodisch via `setSharedOverrides` aktualisiert; offline bleibt
// der zuletzt geladene Stand erhalten (Rust-Cache liefert ihn nach).
let sharedOverrides: NameOverride[] = [];

/** Setzt das geteilte Aussprache-Wörterbuch (von der Community-DB). */
export function setSharedOverrides(list: NameOverride[]): void {
  sharedOverrides = Array.isArray(list) ? list : [];
}

function buildOverrideMap(
  userOverrides: NameOverride[] | undefined,
  enabled: boolean,
): Map<string, string> {
  const map = new Map<string, string>();
  if (!enabled) return map;
  // Reihenfolge = Priorität (späterer Eintrag überschreibt): Basis < geteilt <
  // Nutzer. So gewinnen eigene Korrekturen, dann die Community, dann die Basis.
  for (const o of [
    ...BASE_NAME_OVERRIDES,
    ...sharedOverrides,
    ...(userOverrides ?? []),
  ]) {
    const key = normalizeName(o.name);
    const say = (o.say ?? "").trim();
    if (key && say) map.set(key, say);
  }
  return map;
}

// Wendet die Aussprache-Korrektur auf EINEN Spielernamen an. Reihenfolge je
// Name/Token: 1) exakter Voll-Name-Treffer im Wörterbuch/Tabelle, sonst pro
// Wort: 2) Wörterbuch-/Tabellen-Treffer, 3) Regel-Engine (nur wenn der Name per
// markantem chinesischem/vietnamesischem Nachnamen erkannt wurde), 4) sonst
// unverändert. Whitespace bleibt erhalten. `engine=false` (Korrektur aus) →
// Name 1:1.
function applyOverride(
  name: string,
  map: Map<string, string>,
  engine: boolean,
): string {
  if (!engine && map.size === 0) return name;
  const full = map.get(normalizeName(name));
  if (full) return full;
  // Sprache aus den markanten Nachnamen ableiten (nur für die Regel-Engine).
  const lang = engine
    ? detectNameLang(name.split(/\s+/).filter(Boolean))
    : null;
  return name
    .split(/(\s+)/)
    .map((tok) => {
      if (!/\S/.test(tok)) return tok; // Whitespace unverändert
      // Für den Lookup an Wort-Rändern hängende Satzzeichen ignorieren.
      const stripped = tok.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
      const hit = map.get(normalizeName(stripped));
      if (hit) return hit; // Wörterbuch/Tabelle gewinnt
      if (lang && stripped) return transliterateToken(stripped, lang); // Regel-Engine
      return tok;
    })
    .join("");
}

function joinNames(
  names: string[],
  lang: AnnounceLang,
  overrides: Map<string, string>,
  engine: boolean,
): string {
  const clean = names
    .map((n) => applyOverride(n, overrides, engine).trim())
    .filter((n) => n.length > 0);
  if (clean.length === 0) return "";
  if (clean.length === 1) return clean[0];
  const connector = lang === "de" ? " und " : " and ";
  return clean.slice(0, -1).join(", ") + connector + clean[clean.length - 1];
}

// Liefert das anzusagende Runden-Label — NUR ab Viertelfinale (Viertel-/Halb-
// finale, Finale, Spiel um Platz 3). Frühere Runden, Gruppen, Achtelfinale →
// `null` (nicht ansagen). `roundName` ist die rohe BTP-Runde (z. B. "VF", "HF",
// "Finale", "Spiel um Platz 3", aber auch "G1", "Achtelfinale", "1. Runde").
export function knockoutRoundLabel(
  roundName: string | undefined,
  lang: AnnounceLang,
): string | null {
  const r = (roundName ?? "")
    .toLowerCase()
    .replace(/[.\-_/]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!r) return null;
  const k = r.replace(/\s/g, "");
  // Frühe Runden / Gruppen / Achtelfinale explizit NICHT ansagen.
  // (1) Substring-Treffer (dürfen irgendwo in k stehen):
  if (
    /(achtel|sechzehntel|runde[0-9]|round[0-9]|roundof|gruppe|group|pool|quali|vorrunde)/.test(
      k,
    )
  ) {
    return null;
  }
  // (2) Anker-Treffer (nur am Stringanfang relevant, z. B. "G1", "Pos 5-8"):
  if (/^(g[0-9]|pos)/.test(k)) {
    return null;
  }
  if (/(umplatz3|platz3|3platz|bronze|thirdplace|3rdplace|^3rd)/.test(k)) {
    return lang === "de" ? "Spiel um Platz 3" : "Third place match";
  }
  if (/(viertelfinale|quarterfinal|^vf$|^qf$)/.test(k)) {
    return lang === "de" ? "Viertelfinale" : "Quarterfinal";
  }
  if (/(halbfinale|semifinale|semifinal|^hf$|^sf$)/.test(k)) {
    return lang === "de" ? "Halbfinale" : "Semifinal";
  }
  if (/(^finale$|^final$|^f$|endspiel)/.test(k)) {
    return lang === "de" ? "Finale" : "Final";
  }
  return null;
}

// Baut die Ansage als Liste kurzer Segmente: Gong → Feld → Disziplin →
// (Runde ab Viertelfinale) → Paarung → Feld. Jedes Segment ist eine eigene
// Utterance — Browser-TTS spricht kurze Stücke deutlich klarer.
export function buildAnnouncementSegments(
  input: AnnounceMatchInput,
  lang: AnnounceLang,
  nameOverrides?: NameOverride[],
  nameOverridesEnabled = true,
): string[] {
  const overrides = buildOverrideMap(nameOverrides, nameOverridesEnabled);
  const court = resolveCourtPhrase(input.courtLabel, lang);
  const teamA = joinNames(
    input.teamANames,
    lang,
    overrides,
    nameOverridesEnabled,
  );
  const teamB = joinNames(
    input.teamBNames,
    lang,
    overrides,
    nameOverridesEnabled,
  );
  const versus = lang === "de" ? "gegen" : "versus";
  const disc = disciplineWithClass(input.discipline, input.className, lang);
  const round = knockoutRoundLabel(input.roundName, lang);

  const segments: string[] = [`${court}.`];
  if (disc) segments.push(`${disc}.`);
  // Runde ab Viertelfinale vor der Paarung ansagen (wertet die Ansage auf).
  if (round) segments.push(`${round}.`);
  if (teamA) segments.push(`${teamA}.`);
  if (teamB) segments.push(`${versus} ${teamB}.`);
  segments.push(`${court}.`);
  return segments;
}

// ─── Azure-SSML (hochwertige Ansage am Stück) ────────────────────────────
function xmlEscape(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

// IPA-Lookup-Map (normalisierter Name → IPA-Phoneme) für den Azure-Pfad.
// Quelle: geteiltes Wörterbuch (Lexikon/Community) + Nutzer-Tabelle; das
// mitgelieferte Basis-Wörterbuch hat kein IPA. Leer, wenn Korrektur aus.
function buildIpaMap(
  userOverrides: NameOverride[] | undefined,
  enabled: boolean,
): Map<string, string> {
  const map = new Map<string, string>();
  if (!enabled) return map;
  for (const o of [...sharedOverrides, ...(userOverrides ?? [])]) {
    const ipa = (o.ipa ?? "").trim();
    const key = normalizeName(o.name);
    if (key && ipa) map.set(key, ipa);
  }
  return map;
}

// Manuelle Sprach-Korrektur (normalisierter Voll-Name → `lang`-Wert: ""=auto
// ausgelassen, "de"=deutscher Default erzwingen, sonst NameLang). Nur aus der
// Nutzer-Tabelle (bewusste Einzelfall-Korrektur). Leer, wenn Korrektur aus.
function buildLangOverrideMap(
  userOverrides: NameOverride[] | undefined,
  enabled: boolean,
): Map<string, string> {
  const map = new Map<string, string>();
  if (!enabled) return map;
  for (const o of userOverrides ?? []) {
    const lang = (o.lang ?? "").trim();
    const key = normalizeName(o.name);
    if (key && lang) map.set(key, lang);
  }
  return map;
}

// Ein Wort als Azure-`<phoneme>` mit IPA aussprechen (präziseste Korrektur).
function phonemeSsml(text: string, ipa: string): string {
  return `<phoneme alphabet="ipa" ph="${xmlEscape(ipa)}">${xmlEscape(text)}</phoneme>`;
}

// Erkannte Herkunftssprache → Azure-Locale für den `<lang>`-Tag.
const LANG_LOCALE: Record<NameLang, string> = {
  cn: "zh-CN",
  vn: "vi-VN",
  es: "es-ES",
  fr: "fr-FR",
  pl: "pl-PL",
  tr: "tr-TR",
  ms: "ms-MY",
  in: "en-IN",
};

// Fallback ohne IPA: Namen in seiner erkannten Sprache als `<lang>`-Span hüllen,
// damit die mehrsprachige Azure-Stimme ihn nativ spricht. Nur bei eindeutiger
// Erkennung (`detectNameLang`) — sonst roh (deutscher Default).
function langWrapSsml(name: string): string {
  // Für die Erkennung an Wort-Rändern hängende Satzzeichen entfernen (sonst
  // verfehlt der Wörterbuch-Lookup, z. B. „Garcia," → „garcia,").
  const toks = name
    .split(/\s+/)
    .map((t) => t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter(Boolean);
  const l = detectNameLang(toks);
  const tag = l ? LANG_LOCALE[l] : null;
  return tag
    ? `<lang xml:lang="${tag}">${xmlEscape(name)}</lang>`
    : xmlEscape(name);
}

// Einen Spielernamen für Azure aufbereiten. Reihenfolge: 1) ganzer Name im
// IPA-Wörterbuch → `<phoneme>`; 2) sonst wortweise IPA-Treffer als `<phoneme>`,
// Rest mit `<lang>`-Erkennung; 3) ohne IPA-Map nur `<lang>`-Erkennung.
function nameSsml(
  name: string,
  ipaMap?: Map<string, string>,
  langMap?: Map<string, string>,
): string {
  // 1) Manuelle Sprach-Korrektur (Voll-Name) hat Vorrang: "de" = deutscher
  //    Default (kein Tag), sonst die erzwungene Sprache als `<lang>`.
  const lo = langMap?.get(normalizeName(name));
  if (lo) {
    if (lo === "de") return xmlEscape(name);
    const loc = LANG_LOCALE[lo as NameLang];
    if (loc) return `<lang xml:lang="${loc}">${xmlEscape(name)}</lang>`;
  }
  // 2) Kuratiertes IPA (ganzer Name).
  const full = ipaMap?.get(normalizeName(name));
  if (full) return phonemeSsml(name, full);
  // 3) Wortweise IPA-Treffer, Rest mit `<lang>`-Erkennung.
  if (ipaMap && ipaMap.size > 0) {
    return name
      .split(/(\s+)/)
      .map((tok) => {
        if (!/\S/.test(tok)) return tok; // Whitespace unverändert
        const stripped = tok.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
        const hit = ipaMap.get(normalizeName(stripped));
        return hit ? phonemeSsml(tok, hit) : langWrapSsml(tok);
      })
      .join("");
  }
  // 4) Reine `<lang>`-Erkennung.
  return langWrapSsml(name);
}

function joinNamesSsml(
  names: string[],
  lang: AnnounceLang,
  ipaMap?: Map<string, string>,
  langMap?: Map<string, string>,
): string {
  const clean = names.map((n) => n.trim()).filter((n) => n.length > 0);
  if (clean.length === 0) return "";
  if (clean.length === 1) return nameSsml(clean[0], ipaMap, langMap);
  const connector = lang === "de" ? " und " : " and ";
  return (
    clean
      .slice(0, -1)
      .map((n) => nameSsml(n, ipaMap, langMap))
      .join(", ") +
    connector +
    nameSsml(clean[clean.length - 1], ipaMap, langMap)
  );
}

// Baut die ganze Feld-Ansage als EIN SSML für Azure: deutscher/englischer
// Rahmen (Feld/Disziplin/Runde/„gegen") + Namen je in ihrer erkannten Sprache.
export function buildAnnouncementSsml(
  input: AnnounceMatchInput,
  lang: AnnounceLang,
  voice: string,
  ipaMap?: Map<string, string>,
  langMap?: Map<string, string>,
): string {
  const court = xmlEscape(resolveCourtPhrase(input.courtLabel, lang));
  const disc = xmlEscape(
    disciplineWithClass(input.discipline, input.className, lang),
  );
  const round = knockoutRoundLabel(input.roundName, lang);
  const versus = lang === "de" ? "gegen" : "versus";
  const teamA = joinNamesSsml(input.teamANames, lang, ipaMap, langMap);
  const teamB = joinNamesSsml(input.teamBNames, lang, ipaMap, langMap);

  const parts: string[] = [`${court}.`];
  if (disc) parts.push(`${disc}.`);
  if (round) parts.push(`${xmlEscape(round)}.`);
  if (teamA) parts.push(`${teamA}.`);
  if (teamB) parts.push(`${versus} ${teamB}.`);
  parts.push(`${court}.`);

  const speakLang = lang === "de" ? "de-DE" : "en-US";
  return (
    `<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="${speakLang}">` +
    `<voice name="${xmlEscape(voice)}">${parts.join(" ")}</voice></speak>`
  );
}

// Spielt Gong + spricht die Ansage. Wirft NICHT bei fehlendem
// SpeechSynthesis-Support, läuft dann nur als Gong durch.
export function playAnnouncement(
  input: AnnounceMatchInput,
  lang: AnnounceLang,
  opts: AnnounceOptions = {},
): Promise<void> {
  return enqueueAnnouncement(async () => {
    await maybeGong(opts.gong);
    // Hochwertiger Azure-Weg (ganze Ansage am Stück); bei Fehler → Web Speech.
    if (opts.azure) {
      try {
        const enabled = opts.nameOverridesEnabled ?? true;
        const ipaMap = buildIpaMap(opts.nameOverrides, enabled);
        const langMap = buildLangOverrideMap(opts.nameOverrides, enabled);
        const b64 = await opts.azure.synthesize(
          buildAnnouncementSsml(input, lang, opts.azure.voice, ipaMap, langMap),
        );
        await playMp3Base64(b64);
        return;
      } catch (e) {
        // Azure aus/Netzfehler → Fallback unten, aber sichtbar (Banner),
        // damit der Rückfall auf die Standardstimme nicht mehr stumm passiert.
        reportAzureFallback(e);
      }
    }
    await speakSegments(
      buildAnnouncementSegments(
        input,
        lang,
        opts.nameOverrides,
        opts.nameOverridesEnabled ?? true,
      ),
      lang,
      clampRate(opts.rate),
      opts.voiceURI,
    );
  });
}

// Spielt Gong + spricht einen FREIEN Text (manuelle Ansage). Azure, wenn
// konfiguriert (Text 1:1 in SSML), sonst Web Speech. Strikt sequenziell über
// dieselbe Warteschlange wie die Spiel-Ansagen.
export function playFreeText(
  text: string,
  lang: AnnounceLang,
  opts: AnnounceOptions = {},
): Promise<void> {
  const t = (text || "").trim();
  if (!t) return Promise.resolve();
  return enqueueAnnouncement(async () => {
    // Freitext = kein Spielaufruf → anderer (aufsteigender) Gong.
    await maybeGong(opts.gong, "info");
    if (opts.azure) {
      try {
        const speakLang = lang === "de" ? "de-DE" : "en-US";
        const ssml =
          `<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="${speakLang}">` +
          `<voice name="${xmlEscape(opts.azure.voice)}">${xmlEscape(t)}</voice></speak>`;
        const b64 = await opts.azure.synthesize(ssml);
        await playMp3Base64(b64);
        return;
      } catch (e) {
        reportAzureFallback(e); // → Web Speech unten, mit sichtbarem Hinweis
      }
    }
    await speakSegments([t], lang, clampRate(opts.rate), opts.voiceURI);
  });
}

// Test-Ansage für die Einstellungen — feste Beispieldaten, damit der Klang
// vor dem Turnier prüfbar ist (und der Klick das WebView2-Audio entsperrt).
export async function playTestAnnouncement(
  lang: AnnounceLang,
  opts: AnnounceOptions = {},
): Promise<void> {
  await playAnnouncement(
    {
      courtLabel: "2",
      discipline: "mens_doubles",
      // Beispiel-Klasse, damit der Test das neue Format hörbar macht.
      className: "A",
      teamANames: ["Anna Müller", "Bert Klein"],
      teamBNames: ["Clara Wolf", "Dirk Stein"],
    },
    lang,
    opts,
  );
}

// Spricht NUR einen einzelnen Text (Name) – ohne Gong, ohne Feld/Disziplin.
// Für den Test-Knopf je Aussprache-Korrektur in den Einstellungen: man hört
// sofort, wie die eingetragene Ersatz-Schreibweise klingt, und justiert nach.
export function playNameTest(
  text: string,
  lang: AnnounceLang,
  opts: AnnounceOptions = {},
): Promise<void> {
  return enqueueAnnouncement(() =>
    speakSegments([text.trim()], lang, clampRate(opts.rate), opts.voiceURI),
  );
}

// Stoppt alle laufenden Ansagen UND verwirft noch wartende — z. B. wenn die
// Ansagen abgeschaltet werden. `announceGen` hochzählen entwertet bereits
// eingereihte Aufgaben (sie werden übersprungen statt noch abgespielt).
export function cancelAnnouncements(): void {
  announceGen++;
  announceQueue = Promise.resolve();
  // Laufendes Azure-Audio stoppen (Pendant zu speechSynthesis.cancel()).
  try {
    activeAzureSrc?.stop();
  } catch {
    /* bereits beendet */
  }
  activeAzureSrc = null;
  if (typeof window.speechSynthesis !== "undefined") {
    window.speechSynthesis.cancel();
  }
}

// ─── Vorbereitungs-Ansage ────────────────────────────────────────────────
//
// Eigene Variante für „Spiele in Vorbereitung". Anders als die Feld-Ansage
// trägt sie keinen Court — der Aufruf gilt einem Hallen-Display, nicht
// einem Spielfeld. Stattdessen optional die Halle, in die gerufen wurde.

export interface AnnouncePreparationInput {
  /** Disziplin des Matches. */
  discipline: Discipline;
  /** Spieler-Namen Team A (1 bei Einzel, 2 bei Doppel/Mixed). */
  teamANames: string[];
  /** Spieler-Namen Team B. */
  teamBNames: string[];
  /** Halle, in die gerufen wurde (BTP-`Location`-Name). Leer/undefined =
   *  hallenunabhängiger Aufruf (Ein-Hallen-Turnier). */
  hall?: string;
  /** Klassen-Kürzel („A", „B", …) — direkt hinter der Disziplin angesagt. */
  className?: string;
  /** Reine BTP-Runde; wird ab Viertelfinale vor der Paarung mitangesagt. */
  roundName?: string;
  /** Aufruf-Stufe: 1 = erster Aufruf („In Vorbereitung"), 2 = „Zweiter
   *  Aufruf", 3 = „Dritter und letzter Aufruf". Bei 2/3 wird meist nur die
   *  fehlende Partei genannt (teamBNames leer lassen). Default 1. */
  callStage?: 1 | 2 | 3;
}

// Führt-Präfix je Aufruf-Stufe (an Tilos BTS angelehnt). null = Stufe 1
// (normaler „In Vorbereitung"-Aufruf). Ab Stufe 2 leitet der Text mit
// „Zweiter/Dritter … Aufruf für:" ein, danach folgen die Partei-Namen.
function callStagePrefix(
  stage: 1 | 2 | 3 | undefined,
  lang: AnnounceLang,
): string | null {
  if (stage === 3) {
    return lang === "de"
      ? "Dritter und letzter Aufruf für:"
      : "Third and final call for:";
  }
  if (stage === 2) {
    return lang === "de" ? "Zweiter Aufruf für:" : "Second call for:";
  }
  return null;
}

// Baut die Vorbereitungs-Ansage als Liste kurzer Segmente: „In
// Vorbereitung" → Disziplin → Paarung → (Halle). Jedes Segment ist eine
// eigene Utterance, das macht natürliche Pausen wie bei der Feld-Ansage.
export function buildPreparationSegments(
  input: AnnouncePreparationInput,
  lang: AnnounceLang,
  nameOverrides?: NameOverride[],
  nameOverridesEnabled = true,
): string[] {
  const overrides = buildOverrideMap(nameOverrides, nameOverridesEnabled);
  const teamA = joinNames(
    input.teamANames,
    lang,
    overrides,
    nameOverridesEnabled,
  );
  const teamB = joinNames(
    input.teamBNames,
    lang,
    overrides,
    nameOverridesEnabled,
  );
  const versus = lang === "de" ? "gegen" : "versus";
  const disc = disciplineWithClass(input.discipline, input.className, lang);
  const round = knockoutRoundLabel(input.roundName, lang);
  const hall = (input.hall || "").trim();
  const stagePrefix = callStagePrefix(input.callStage, lang);

  // Wiederholungsaufruf (Stufe 2/3): terse „Zweiter/Dritter Aufruf für:
  // {Partei}. Bitte in {Halle}." — kein „In Vorbereitung", keine Disziplin
  // (wie Tilos gezielter Zweitaufruf; genannt wird nur die fehlende Partei).
  if (stagePrefix) {
    const segments: string[] = [stagePrefix];
    if (teamA && teamB) {
      segments.push(`${teamA}.`);
      segments.push(`${versus} ${teamB}.`);
    } else if (teamA) {
      segments.push(`${teamA}.`);
    }
    if (hall) {
      segments.push(
        lang === "de" ? `Bitte in ${hall}.` : `Please report to ${hall}.`,
      );
    }
    return segments;
  }

  const segments: string[] = [
    lang === "de" ? "In Vorbereitung." : "Preparation call.",
  ];
  if (disc) segments.push(`${disc}.`);
  if (round) segments.push(`${round}.`);
  // Beide Teams oder nur Team A — ein Solo-„gegen TeamB" ohne Subjekt
  // wäre grammatikalisch kaputt; in dem Fall lieber gar keine Paarung.
  if (teamA && teamB) {
    segments.push(`${teamA}.`);
    segments.push(`${versus} ${teamB}.`);
  } else if (teamA) {
    segments.push(`${teamA}.`);
  }
  if (hall) {
    segments.push(
      lang === "de" ? `Bitte in ${hall}.` : `Please report to ${hall}.`,
    );
  }
  return segments;
}

// Vorbereitungs-Ansage als EIN SSML für Azure (Namen je in ihrer Sprache).
export function buildPreparationSsml(
  input: AnnouncePreparationInput,
  lang: AnnounceLang,
  voice: string,
  ipaMap?: Map<string, string>,
  langMap?: Map<string, string>,
): string {
  const disc = xmlEscape(
    disciplineWithClass(input.discipline, input.className, lang),
  );
  const round = knockoutRoundLabel(input.roundName, lang);
  const versus = lang === "de" ? "gegen" : "versus";
  const teamA = joinNamesSsml(input.teamANames, lang, ipaMap, langMap);
  const teamB = joinNamesSsml(input.teamBNames, lang, ipaMap, langMap);
  const hall = xmlEscape((input.hall || "").trim());
  const stagePrefix = callStagePrefix(input.callStage, lang);

  let parts: string[];
  if (stagePrefix) {
    // Wiederholungsaufruf (Stufe 2/3) — terse, nur die genannte Partei.
    parts = [xmlEscape(stagePrefix)];
    if (teamA && teamB) {
      parts.push(`${teamA}.`);
      parts.push(`${versus} ${teamB}.`);
    } else if (teamA) {
      parts.push(`${teamA}.`);
    }
    if (hall) {
      parts.push(
        lang === "de" ? `Bitte in ${hall}.` : `Please report to ${hall}.`,
      );
    }
  } else {
    parts = [lang === "de" ? "In Vorbereitung." : "Preparation call."];
    if (disc) parts.push(`${disc}.`);
    if (round) parts.push(`${xmlEscape(round)}.`);
    if (teamA && teamB) {
      parts.push(`${teamA}.`);
      parts.push(`${versus} ${teamB}.`);
    } else if (teamA) {
      parts.push(`${teamA}.`);
    }
    if (hall) {
      parts.push(
        lang === "de" ? `Bitte in ${hall}.` : `Please report to ${hall}.`,
      );
    }
  }
  const speakLang = lang === "de" ? "de-DE" : "en-US";
  return (
    `<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="${speakLang}">` +
    `<voice name="${xmlEscape(voice)}">${parts.join(" ")}</voice></speak>`
  );
}

// Spielt Gong + spricht die Vorbereitungs-Ansage. Wirft NICHT bei
// fehlendem SpeechSynthesis-Support, läuft dann nur als Gong durch.
export function playPreparationAnnouncement(
  input: AnnouncePreparationInput,
  lang: AnnounceLang,
  opts: AnnounceOptions = {},
): Promise<void> {
  return enqueueAnnouncement(async () => {
    await maybeGong(opts.gong);
    if (opts.azure) {
      try {
        const enabled = opts.nameOverridesEnabled ?? true;
        const ipaMap = buildIpaMap(opts.nameOverrides, enabled);
        const langMap = buildLangOverrideMap(opts.nameOverrides, enabled);
        const b64 = await opts.azure.synthesize(
          buildPreparationSsml(input, lang, opts.azure.voice, ipaMap, langMap),
        );
        await playMp3Base64(b64);
        return;
      } catch (e) {
        reportAzureFallback(e); // → Web Speech unten, mit sichtbarem Hinweis
      }
    }
    await speakSegments(
      buildPreparationSegments(
        input,
        lang,
        opts.nameOverrides,
        opts.nameOverridesEnabled ?? true,
      ),
      lang,
      clampRate(opts.rate),
      opts.voiceURI,
    );
  });
}

// Auto-Sprachwahl anhand der Nationalitäten: Englisch, sobald mindestens
// die Hälfte der Spieler international ist (Nationalität gesetzt und
// ≠ GER). Wird von Feld-Ansage und Vorbereitungs-Ansage geteilt.
export function resolveAnnouncementLanguage(
  nationalities: string[],
  mode: AnnounceLanguageMode,
): AnnounceLang {
  if (mode === "de" || mode === "en") return mode;
  if (nationalities.length === 0) return "de";
  const international = nationalities.filter((n) => {
    const code = n.trim().toUpperCase();
    return code !== "" && code !== "GER";
  }).length;
  return international * 2 >= nationalities.length ? "en" : "de";
}
