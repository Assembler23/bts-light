// Die wählbaren Sprachdienste der Ansage (Spec `ansage-elevenlabs`).
//
// Spiegelt Rust `config::dienst_art`; `scripts/test-sprachdienst.mjs` hält
// beide Listen deckungsgleich. Unbekannte Werte gelten — wie im Kern — als
// Azure, damit Frontend und Kern nie verschiedene Dienste annehmen.

/** Wert → Anzeigename + Anbieter, in der Reihenfolge der Auswahl. */
export const DIENSTE = [
  { wert: "azure", name: "Azure", art: "azure" },
  { wert: "elevenlabs_v2", name: "ElevenLabs v2 (multilingual)", art: "elevenlabs" },
  { wert: "elevenlabs_v3", name: "ElevenLabs v3", art: "elevenlabs" },
  { wert: "elevenlabs_v4", name: "ElevenLabs v4", art: "elevenlabs" },
  { wert: "elevenlabs_flash", name: "ElevenLabs Flash v2.5 (günstiger)", art: "elevenlabs" },
  { wert: "openai", name: "OpenAI (gpt-4o-mini-tts)", art: "openai" },
  { wert: "gemini_38_flash", name: "Gemini 3.8 Flash TTS", art: "gemini" },
  { wert: "gemini_38_lite", name: "Gemini 3.8 Flash-Lite TTS (günstiger)", art: "gemini" },
];

/** Alte Werte → Nachfolger (Spiegel von Rust `config::dienst_normalisiert`):
 *  Gemini 2.5 schaltet Google am 17.11.2026 ab. */
const NACHFOLGER = { gemini_flash: "gemini_38_flash", gemini_pro: "gemini_38_flash" };

/** Dienst-Wert mit alten Werten auf ihren Nachfolger gehoben. */
export function dienstNormalisiert(dienst) {
  return NACHFOLGER[dienst] ?? dienst;
}

/** Anbieter zu einem `dienst`-Wert; unbekannt/leer → "azure". */
export function dienstArt(dienst) {
  const w = dienstNormalisiert(dienst);
  return DIENSTE.find((d) => d.wert === w)?.art ?? "azure";
}

/** Anzeigename; unbekannt → "Azure". */
export function dienstName(dienst) {
  const w = dienstNormalisiert(dienst);
  return DIENSTE.find((d) => d.wert === w)?.name ?? "Azure";
}

/** Schlüssel-Feld der Config je Anbieter. */
const SCHLUESSEL = {
  elevenlabs: "elevenlabs_key",
  openai: "openai_key",
  gemini: "gemini_key",
};

/**
 * Ist der GEWÄHLTE Dienst vollständig eingerichtet? Spiegel von Rust
 * `AzureTtsConfig::vollstaendig`; `enabled` gilt für alle Dienste.
 */
export function cloudStimmeVollstaendig(az) {
  if (!az || !az.enabled) return false;
  const art = dienstArt(az.dienst);
  if (art === "azure") return Boolean(az.key?.trim() && az.region?.trim());
  return Boolean(az[SCHLUESSEL[art]]?.trim());
}
