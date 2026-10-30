/** Pause zwischen Disziplin und Klasse im Azure-SSML. */
export const KLASSE_PAUSE_MS: number;
/** Disziplin + Klasse für die Gerätestimme („Herreneinzel, A"). */
export function klasseText(
  disziplin: string | undefined,
  klasse: string | undefined,
): string;
/** Disziplin + Klasse als escaptes SSML-Stück mit Pause. */
export function klasseSsml(
  disziplin: string | undefined,
  klasse: string | undefined,
): string;
