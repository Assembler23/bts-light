export function gongPlanen(
  ctx: BaseAudioContext,
  kind: "match" | "info",
): { letzter: OscillatorNode; endeSec: number };
