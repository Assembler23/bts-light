export function besetzungSegmente(
  sr: string[] | undefined,
  ar: string[] | undefined,
  lang: "de" | "en",
): string[];
export function besetzungAusAuftrag(
  job: { kind: string; sr?: string[]; ar?: string[] },
  besetzungAnsagen: boolean,
): { umpireNames?: string[]; serviceJudgeNames?: string[] };
export function besetzungMitFeld(
  sr: string[] | undefined,
  ar: string[] | undefined,
  lang: "de" | "en",
  feld: string,
): string[];
