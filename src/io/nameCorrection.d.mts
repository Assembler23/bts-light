export interface NameCorrection {
  /** "ipa" → präzises `<phoneme>`; "say" → phonetische Ersatzschreibweise. */
  kind: "ipa" | "say";
  /** IPA-String bzw. Ersatz-Text. */
  value: string;
}

/** Wählt die Aussprache-Korrektur für einen normalisierten Namensschlüssel.
 *  IPA schlägt `say`; ohne Treffer null (Aufrufer → `<lang>`-Erkennung). */
export function resolveNameCorrection(
  key: string,
  ipaMap?: Map<string, string>,
  sayMap?: Map<string, string>,
): NameCorrection | null;
