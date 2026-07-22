/** Wählt die Azure-Stimme für eine Disziplin: die hinterlegte Sonder-Stimme
 *  (`discipline_voices[discipline]`), sonst die Standard-Stimme `baseVoice`. */
export function voiceForDiscipline(
  baseVoice: string,
  disciplineVoices: Record<string, string> | undefined,
  discipline: string | undefined,
): string;
