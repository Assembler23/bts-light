// Typdeklaration für den Produktiv-/Test-Umschalter (badhubZiel.mjs).

export declare const BADHUB_HOST_LIVE: string;
export declare const BADHUB_HOST_TEST: string;

/** Der badhub-Host für das gewählte System. */
export declare function badhubHostFuer(test: boolean): string;

/** Lässt sich diese Adresse umschalten? Nur badhub.de/test.badhub.de. */
export declare function istUmschaltbar(url: string | undefined): boolean;

/** Zeigt diese URL auf das Testsystem? Fremde Hosts gelten als Produktiv. */
export declare function istTestsystem(url: string | undefined): boolean;

/** Biegt eine badhub-URL auf Produktiv oder Test um; fremde Hosts bleiben. */
export declare function badhubUrlFuer(
  url: string | undefined,
  test: boolean,
): string;

/** Schaltet einen kompletten Badhub-Zugang um (Push-URL + Live-Seite). */
export declare function badhubZielFuer<
  T extends { url: string; live_url: string },
>(badhub: T, test: boolean): T;
