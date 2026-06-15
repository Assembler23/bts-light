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
import { BASE_NAME_OVERRIDES } from "./nameOverrideBase";

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
}

// Synthesizer-Gong über Web Audio. Zwei kurze Sinus-Töne (hoch → tiefer) mit
// kleinem Decay, ähnlich einem Hotel-Gong. Liefert eine Promise, die
// resolved, wenn der Gong durchgespielt ist — damit die Sprache erst danach
// startet.
async function playGong(ctx: AudioContext): Promise<void> {
  const now = ctx.currentTime;
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(0.4, now + 0.05);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + 1.2);
  gain.connect(ctx.destination);

  const o1 = ctx.createOscillator();
  o1.type = "sine";
  o1.frequency.value = 880; // A5
  o1.connect(gain);
  o1.start(now);
  o1.stop(now + 0.6);

  const o2 = ctx.createOscillator();
  o2.type = "sine";
  o2.frequency.value = 587.33; // D5
  o2.connect(gain);
  o2.start(now + 0.18);
  o2.stop(now + 1.1);

  // Erst auflösen, wenn die Gain-Hülle ausgeklungen ist (~1,2 s) – sonst
  // setzt die Sprachausgabe noch in den Gong-Nachklang ein.
  return new Promise((resolve) => {
    setTimeout(resolve, 1250);
  });
}

// Reusable AudioContext — Browser-Limit von 1–6 contexts pro Tab; einer reicht.
let cachedCtx: AudioContext | null = null;
function getAudioContext(): AudioContext {
  if (cachedCtx == null) {
    cachedCtx = new (window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext })
        .webkitAudioContext)();
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

// ─── Globale Ansage-Warteschlange ────────────────────────────────────────
// ALLE Ansagen (Feld-Auto-Ansage, manuelle Ansage, Vorbereitung) laufen
// strikt nacheinander durch DIESE eine Kette — so kann nie ein Gong starten,
// während eine vorige Ansage noch spricht (Feld-Bug 2026-06-14). Eine
// „Generation" entwertet noch wartende Aufgaben, wenn abgebrochen wird.
let announceQueue: Promise<void> = Promise.resolve();
let announceGen = 0;

function enqueueAnnouncement(task: () => Promise<void>): Promise<void> {
  const gen = announceGen;
  const run = announceQueue.then(() => (gen === announceGen ? task() : undefined));
  // Kette auch nach einem Fehler weiterlaufen lassen.
  announceQueue = run.then(
    () => {},
    () => {},
  );
  return run;
}

// Spielt (optional) den Gong und wartet, bis er ausgeklungen ist.
async function maybeGong(gong: boolean | undefined): Promise<void> {
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
    await playGong(ctx);
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
  "", "eins", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht",
  "neun", "zehn", "elf", "zwölf", "dreizehn", "vierzehn", "fünfzehn",
  "sechzehn", "siebzehn", "achtzehn", "neunzehn", "zwanzig",
];
const NUMBER_WORDS_EN = [
  "", "one", "two", "three", "four", "five", "six", "seven", "eight",
  "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen",
  "sixteen", "seventeen", "eighteen", "nineteen", "twenty",
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
function buildOverrideMap(
  userOverrides: NameOverride[] | undefined,
  enabled: boolean,
): Map<string, string> {
  const map = new Map<string, string>();
  if (!enabled) return map;
  for (const o of [...BASE_NAME_OVERRIDES, ...(userOverrides ?? [])]) {
    const key = normalizeName(o.name);
    const say = (o.say ?? "").trim();
    if (key && say) map.set(key, say);
  }
  return map;
}

// Wendet die Aussprache-Korrekturen auf EINEN Spielernamen an: zuerst ein
// exakter Voll-Name-Treffer, sonst Wort für Wort (so wirkt z. B. ein einmal
// eingetragener Nachname „Nguyen" bei allen Spieler:innen mit diesem Namen).
// Whitespace bleibt erhalten; Nicht-Treffer bleiben unverändert.
function applyOverride(name: string, map: Map<string, string>): string {
  if (map.size === 0) return name;
  const full = map.get(normalizeName(name));
  if (full) return full;
  return name
    .split(/(\s+)/)
    .map((tok) => {
      // Für den Lookup an Wort-Rändern hängende Satzzeichen ignorieren
      // (z. B. „Nguyen,") — der unveränderte Token bleibt bei Nicht-Treffer.
      const stripped = tok.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
      return map.get(normalizeName(stripped)) ?? tok;
    })
    .join("");
}

function joinNames(
  names: string[],
  lang: AnnounceLang,
  overrides: Map<string, string>,
): string {
  const clean = names
    .map((n) => applyOverride(n, overrides).trim())
    .filter((n) => n.length > 0);
  if (clean.length === 0) return "";
  if (clean.length === 1) return clean[0];
  const connector = lang === "de" ? " und " : " and ";
  return clean.slice(0, -1).join(", ") + connector + clean[clean.length - 1];
}

// Baut die Ansage als Liste kurzer Segmente: Gong → Feld → Disziplin →
// Paarung → Feld. Jedes Segment ist eine eigene Utterance — Browser-TTS
// spricht kurze Stücke deutlich klarer und macht natürliche Pausen.
export function buildAnnouncementSegments(
  input: AnnounceMatchInput,
  lang: AnnounceLang,
  nameOverrides?: NameOverride[],
  nameOverridesEnabled = true,
): string[] {
  const overrides = buildOverrideMap(nameOverrides, nameOverridesEnabled);
  const court = resolveCourtPhrase(input.courtLabel, lang);
  const teamA = joinNames(input.teamANames, lang, overrides);
  const teamB = joinNames(input.teamBNames, lang, overrides);
  const versus = lang === "de" ? "gegen" : "versus";
  const disc = disciplineWord(input.discipline, lang);

  const segments: string[] = [`${court}.`];
  if (disc) segments.push(`${disc}.`);
  if (teamA) segments.push(`${teamA}.`);
  if (teamB) segments.push(`${versus} ${teamB}.`);
  segments.push(`${court}.`);
  return segments;
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
  const teamA = joinNames(input.teamANames, lang, overrides);
  const teamB = joinNames(input.teamBNames, lang, overrides);
  const versus = lang === "de" ? "gegen" : "versus";
  const disc = disciplineWord(input.discipline, lang);
  const hall = (input.hall || "").trim();

  const segments: string[] = [
    lang === "de" ? "In Vorbereitung." : "Preparation call.",
  ];
  if (disc) segments.push(`${disc}.`);
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

// Spielt Gong + spricht die Vorbereitungs-Ansage. Wirft NICHT bei
// fehlendem SpeechSynthesis-Support, läuft dann nur als Gong durch.
export function playPreparationAnnouncement(
  input: AnnouncePreparationInput,
  lang: AnnounceLang,
  opts: AnnounceOptions = {},
): Promise<void> {
  return enqueueAnnouncement(async () => {
    await maybeGong(opts.gong);
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
