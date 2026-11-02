/** Pause zwischen zwei Blöcken der Ansage. */
export const BLOCK_PAUSE_MS: number;
/** Kurze Pause vor und nach einem fremdsprachigen Namen. */
export const LANG_PAUSE_MS: number;
/** Tempo 0,5–1,5 → relativer Azure-Wert („-20%"); `null` bei Normaltempo. */
export function tempoProzent(rate: number | undefined | null): string | null;
/** Fremdsprachige Namen (`<lang>`) mit kurzer Pause absetzen. */
export function langAbsetzen(inner: string): string;
/** Tempo in ein SSML-Inneres einziehen, ohne `<lang>` in `<prosody>` zu legen. */
export function mitTempo(inner: string, rate: number | undefined | null): string;
/** Das ganze `<speak>`-Dokument aus fertigen SSML-Blöcken. */
export function ssmlDokument(
  lang: string,
  voice: string,
  teile: string[],
  rate: number | undefined | null,
): string;
