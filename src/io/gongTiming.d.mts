// Typdeklaration für die reine Gong-Timing-Logik (gongTiming.mjs). Wird von
// announcer.ts importiert; die Laufzeit-Implementierung liegt im .mjs, damit
// derselbe Code in Node getestet werden kann.

export declare const GONG_BREATH_MS: number;

export declare function gongResolveRace(p: {
  /** Meldet das echte Audio-Ende: ruft den übergebenen Callback beim `onended`. */
  subscribeEnded: (cb: () => void) => void;
  /** Fallback-Frist in ms, falls `onended` ausbleibt. */
  fallbackMs: number;
  /** Atempause nach dem Ende (Default GONG_BREATH_MS). */
  breathMs?: number;
  /** Injizierbarer Timer (Tests). */
  setTimeoutFn?: (cb: () => void, ms: number) => unknown;
}): Promise<void>;
