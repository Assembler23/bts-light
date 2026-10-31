// Typdeklaration für die reine Tonweg-Logik (tonWeg.mjs). Wird von
// announcer.ts importiert; die Laufzeit liegt im .mjs, damit Node sie testet.

export type TonFehlerCode =
  | "nicht_unterstuetzt"
  | "kein_geraet"
  | "keine_stimme"
  | "abgebrochen"
  | "ungueltig"
  | "intern";

export type TonBanner = "kein_geraet" | "keine_stimme" | "rueckfall";

export declare function fehlerCode(err: unknown): TonFehlerCode;

export declare function bannerFuer(
  code: TonFehlerCode | "frist",
): TonBanner | null;

export declare function pcmFristMs(samples: number, rate: number): number;

export declare function stimmeFristMs(segmente: string[], rate: number): number;

export declare function abschnitteTeilen(segmente: string[], max: number): string[];

export type FristErgebnis<T> =
  | { ok: true; wert: T }
  | { ok: false; code: TonFehlerCode | "frist" };

export declare function mitFrist<T>(
  promise: Promise<T>,
  ms: number,
  uhr?: {
    setTimeoutFn?: (cb: () => void, ms: number) => unknown;
    clearTimeoutFn?: (id: unknown) => void;
  },
): Promise<FristErgebnis<T>>;
