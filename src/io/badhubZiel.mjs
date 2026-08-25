// Umschalter zwischen dem Produktiv- und dem Testsystem von badhub.
//
// Schwester der Rust-Seite (`src-tauri/src/badhub_host.rs`) — beide leiten den
// Modus aus der Push-URL ab, statt ihn als eigenes Feld zu führen: zwei
// Wahrheiten (Flag + URL) driften auseinander, und ausgerechnet die stille
// Variante („Flag an, URL zeigt auf Produktiv") schriebe Testdaten in den
// echten Liveticker.
//
// Fremde Hosts bleiben unangetastet — wer eine eigene badhub-Instanz
// betreibt, bekommt seine Adresse nicht umgeschrieben.

export const BADHUB_HOST_LIVE = "badhub.de";
export const BADHUB_HOST_TEST = "test.badhub.de";

/** Hosts, die bts-light als „badhub" umschalten darf. */
function istBadhubHost(host) {
  const h = String(host || "").toLowerCase();
  return (
    h === BADHUB_HOST_LIVE ||
    h === `www.${BADHUB_HOST_LIVE}` ||
    h === BADHUB_HOST_TEST
  );
}

/** Der Host für das gewählte System. */
export function badhubHostFuer(test) {
  return test ? BADHUB_HOST_TEST : BADHUB_HOST_LIVE;
}

/** Zeigt diese URL auf das Testsystem? Fremde Hosts gelten als Produktiv —
 *  eine eigene badhub-Instanz ist kein Testlauf. */
export function istTestsystem(url) {
  try {
    return new URL(String(url).trim()).hostname.toLowerCase() === BADHUB_HOST_TEST;
  } catch {
    return false;
  }
}

/** Biegt eine badhub-URL auf das gewählte System um. Pfad, Query und Fragment
 *  bleiben erhalten. Fremde Hosts und unparsbare Eingaben kommen unverändert
 *  zurück. */
export function badhubUrlFuer(url, test) {
  const roh = String(url ?? "");
  let parsed;
  try {
    parsed = new URL(roh.trim());
  } catch {
    return roh;
  }
  if (!istBadhubHost(parsed.hostname)) return roh;
  parsed.hostname = badhubHostFuer(test);
  return parsed.toString();
}

/** Schaltet einen kompletten Badhub-Zugang um (Push-URL + Live-Seite). Das
 *  Passwort bleibt unberührt — ob das Testsystem dasselbe Token akzeptiert,
 *  weiß nur der Server. */
export function badhubZielFuer(badhub, test) {
  return {
    ...badhub,
    url: badhubUrlFuer(badhub.url, test),
    live_url: badhubUrlFuer(badhub.live_url, test),
  };
}
