// Zeitplan im TL-Web (Spec `tl-zeitplan`, ADR 0094): Texte zur geplanten Zeit
// eines Spiels.
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-zeitplan.mjs` prüft beide auf gleiches Verhalten.

/**
 * Planzeit aus BTP (`YYYYMMDDHHMM`, Wandzeit des Turnier-PCs) als Text:
 * „10:20", an einem anderen Tag als `heute` (`YYYYMMDD` vom Turnier-PC) mit
 * Wochentag: „So 09:00".
 *
 * Gelesen werden nur die Ziffern — nie ein `Date` in der Zone des Geräts.
 * Ein Turnierleitungs-Gerät mit anderer Zeitzone zeigt deshalb dieselbe
 * Uhrzeit wie der Turnier-PC. Der Wochentag kommt aus einer UTC-Rechnung
 * über Jahr/Monat/Tag und hängt ebenfalls an keiner Zone. Kennt der Host
 * `heute` nicht (0, fehlt), steht nur die Uhrzeit. Unsinn ergibt "".
 */
export function planZeitText(plannedTime, heute) {
  const n = Number(plannedTime);
  if (!Number.isFinite(n) || n < 100000000000 || n > 999912312359) return "";
  const minute = n % 100;
  const stunde = Math.floor(n / 100) % 100;
  const datum = Math.floor(n / 10000);
  const tag = datum % 100;
  const monat = Math.floor(datum / 100) % 100;
  const jahr = Math.floor(datum / 10000);
  if (minute > 59 || stunde > 23 || monat < 1 || monat > 12 || tag < 1 || tag > 31) return "";
  const uhr = `${String(stunde).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  const h = Number(heute);
  if (!Number.isFinite(h) || h <= 0 || h === datum) return uhr;
  const wochentag = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"][
    new Date(Date.UTC(jahr, monat - 1, tag)).getUTCDay()
  ];
  return `${wochentag} ${uhr}`;
}

/**
 * Abstand des tatsächlichen Starts zur Planzeit: „+18 min", „−4 min",
 * „±0 min". Fehlt die Messung (Walkover, kein Stempel), "".
 */
export function startAbweichungText(mins) {
  if (mins === null || mins === undefined || mins === "") return "";
  const m = Math.trunc(Number(mins));
  if (!Number.isFinite(m)) return "";
  if (m === 0) return "±0 min";
  return `${m > 0 ? "+" : "−"}${Math.abs(m)} min`;
}

/**
 * Was das Panel „Zeitplan" zeigt: `{ ton, text, neben }`.
 *
 * `schedule` ist das Objekt des Turnier-PCs (Bezugsfenster und dessen
 * früheste Prognose), `nowMs` dessen Uhr (`serverNow()`), nie die des Geräts.
 * `ton`: "ok" (im Zeitplan), "spaet", "frueh" oder "neutral".
 *
 * Abweichung = max(Prognose, jetzt) − Planzeit, zur Null hin auf ganze
 * Minuten abgeschnitten. Die Klemme auf „jetzt" braucht es, weil die
 * Prognose die Revision des Zustands bewusst nicht bewegt: Sie kann alt sein,
 * und ein Start in der Vergangenheit hieße in Wahrheit „noch nicht
 * gestartet". Bis ±5 Minuten gilt „Im Zeitplan".
 */
export function zeitplanLage(schedule, nowMs) {
  const neutral = (text, neben) => ({ ton: "neutral", text, neben: neben || "" });
  if (!schedule || typeof schedule !== "object") return neutral("", "");
  const kind = schedule.kind;
  if (kind === "no_plan") return neutral("Keine geplanten Zeiten in BTP");
  if (kind === "done_today") return neutral("Heute keine Spiele mehr");

  const planMs = Number(schedule.planned_ms);
  const hatPlan = Number.isFinite(planMs) && planMs > 0;
  const jetzt = Number(nowMs);
  // Die Uhrzeit des Fensters: nur aus den Ziffern, ohne Tag (es ist heute).
  const fensterUhr = planZeitText(schedule.planned_time, 0);
  const fenster = fensterUhr ? `Nächstes Zeitfenster ${fensterUhr}` : "";
  const TOLERANZ_MS = 5 * 60000;

  // Vor dem ersten Spiel des Tages gibt es noch keinen Rückstand und keinen
  // Vorsprung — bis die Planzeit samt Toleranz verstrichen ist.
  if (!schedule.started && hatPlan && fensterUhr
      && Number.isFinite(jetzt) && jetzt <= planMs + TOLERANZ_MS) {
    return neutral(`Beginn ${fensterUhr}`);
  }
  if (kind === "last_round") return neutral("Letzte Runde läuft");
  if (kind === "off") return neutral("Prognose ist ausgeschaltet", fenster);

  const prognose = Number(schedule.predicted_start_ms);
  if (kind !== "ok" || !hatPlan || !Number.isFinite(prognose) || prognose <= 0
      || !Number.isFinite(jetzt)) {
    return neutral("Noch keine Prognose", fenster);
  }
  const start = Math.max(prognose, jetzt);
  const m = Math.trunc((start - planMs) / 60000);
  // Voraussichtliche Uhrzeit: Planzeit-Ziffern plus Abweichung — so bleibt
  // auch sie von der Zeitzone des Geräts unabhängig.
  let neben = fenster;
  const pt = Number(schedule.planned_time);
  if (fensterUhr && Number.isFinite(pt)) {
    const planMin = (Math.floor(pt / 100) % 100) * 60 + (pt % 100);
    const min = (((planMin + m) % 1440) + 1440) % 1440;
    const uhr = `${String(Math.floor(min / 60)).padStart(2, "0")}:${String(min % 60).padStart(2, "0")}`;
    neben = `${fenster} · voraussichtlich ${schedule.predicted_uncertain ? "ca. " : ""}${uhr}`;
  }
  if (Math.abs(m) <= 5) return { ton: "ok", text: "Im Zeitplan", neben };
  return m > 0
    ? { ton: "spaet", text: `${m} min dahinter`, neben }
    : { ton: "frueh", text: `${-m} min davor`, neben };
}

/**
 * Panels, die unsichtbar bleiben, bis man sie im Profil einschaltet (ADR
 * 0094). Jedes andere Panel, das in einem Profil fehlt, wird sichtbar
 * ergänzt — sonst bliebe ein neues Panel für alle unauffindbar. Dieses hier
 * soll nach dem Update in keiner bestehenden Ansicht ungefragt auftauchen.
 */
export const PANEL_STANDARD_AUS = ["zeitplan"];

/** Sichtbarkeit eines Panels, das im Profil (noch) nicht vorkommt. */
export function panelStandardSichtbar(key) {
  return !PANEL_STANDARD_AUS.includes(key);
}
