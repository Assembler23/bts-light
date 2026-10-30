export function gongPlanen(
  ctx: AudioContext,
  kind: "match" | "info",
): { letzter: OscillatorNode; endeSec: number };
