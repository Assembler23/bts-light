// Abspiel-Kern der Ansage-Seiten (Spec `ansage-im-browser`, E1 + E1b):
// Abfrage des Feeds, strikte Warteschlange, Gong, Audio vom Turnier-PC mit
// Fristen, Gerätestimme als Rückfall, Präsenz. Ohne eigenen DOM-Zugriff —
// alles, was der Browser liefert (fetch, AudioContext, speechSynthesis,
// Timer), kommt über `welt` herein; so laufen dieselben Zeilen auf der Seite
// `/ansage`, im Ansage-Modus der Monitor-Seiten (`?ansage=<Halle>`) und im
// Test mit Attrappen. Die Entscheidungen selbst stehen in `ansageSeite.mjs`,
// der Gong in `gongKlang.mjs`.
//
// Inline-Kopie (ohne `export`, ab der ersten Leerzeile) zwischen den Markern
// `<ansageKern.mjs>` in `assets/ansage.html`, `monitor.html`, `overview.html`
// und `combo.html`; `scripts/test-ansage-kern.mjs` hält alle vier wortgleich.
// Bewusst ES5-Stil (var/function) wie die Monitor-Seiten.
import {
  ANSAGE_TAKT_MS, hallePasst, halleGueltig, neueEintraege, nochAbspielbar,
  stimme, meldetPraesenz, grundText, feedAdresse,
  ANSAGE_VERLAUF_MAX, ansageText, verlaufAufnehmen, wiederholungsAuftrag,
} from "./ansageSeite.mjs";
import { gongPlanen } from "./gongKlang.mjs";

/* Fristen des Audio-Wegs: Ohne sie hielte ein hängender Abruf (WLAN-Loch)
   oder ein ausbleibendes `onended` (Kontext beim Sperren angehalten) die
   Warteschlange für immer an — die Seite sähe verbunden aus und meldete
   Präsenz, spräche aber nie wieder. */
export var AUDIO_LADE_FRIST_MS = 8000;
export var AUDIO_NACHLAUF_MS = 2000;
/* Frist eines Feed-Abrufs. Ohne sie parkte der Takt auf einer toten Leitung
   alle 1,5 s einen weiteren Abruf — auf einer Monitor-Seite blockierten die
   bald die Verbindungen, über die die Anzeige selbst lädt
   (Lehre Monitor-Stillstand 05./06.09.2026). */
export var ANSAGE_FEED_FRIST_MS = 5000;
/* Kennt der Server den Feed nicht (404: Slave-Brücke, sehr alte Fassung),
   fragt die Seite nur noch einmal je Minute nach statt im Takt. */
export var ANSAGE_FEHLT_PAUSE_MS = 60000;

/**
 * Baut den Kern einer Ansage-Seite.
 *
 * `opt`: `{ base, halle, halleFest, welt, beiAenderung, beiHallen, verlauf }`.
 * `base` endet mit `/`. `halleFest` = die Halle steht fest (Monitor-Modus,
 * aus der Adresse) und wird nicht verworfen, wenn das Turnier sie nicht
 * kennt — die Seite soll dann sagen, welche Halle fehlt. `welt` liefert
 * `fetch`, `AudioContext`, `speechSynthesis`, Timer usw. (im Browser
 * `window`). `beiAenderung()` meldet jeden neuen Stand an die Anzeige,
 * `beiHallen(halls)` eine geänderte Hallenliste. `verlauf` = die Seite führt
 * einen Verlauf der letzten gespielten Ansagen samt geladenem Audio zum
 * Wiederholen (nur `/ansage`; die Monitor-Seiten halten kein Audio vor).
 */
export function ansageKern(opt) {
  var w = opt.welt;
  var base = opt.base;
  var halleFest = !!opt.halleFest;
  var melden = opt.beiAenderung || function () {};
  var hallenMelden = opt.beiHallen || function () {};
  var mitVerlauf = !!opt.verlauf;
  var jetzt = function () { return w.Date.now(); };

  var z = {
    halle: String(opt.halle || ""),
    halls: [],
    hallenGesehen: false,
    feed: null,
    stand: { lastId: 0, bestand: false },
    verbunden: false,
    // Der Server kennt den Feed nicht (404) — seit wann.
    fehltSeit: 0,
    // Gescheiterte Abrufe in Folge — ein einzelner Aussetzer ist kein
    // Grund für einen Hinweis am TV.
    fehlversuche: 0,
    abrufLaeuft: false,
    warteschlange: [],
    spielt: false,
    ctx: null,
    tonFrei: false,
    // Zuletzt gespielte Ansage: `{ ms, text }`.
    letzte: null,
    // Die letzten gespielten Ansagen, neueste vorn (nur mit `verlauf`):
    // `{ id, ms, text, e, puffer }` — `puffer` ist das geladene Audio, das
    // auf dem Turnier-PC nach einer Minute verfällt. Lebt nur im Speicher
    // der offenen Seite (Spielernamen gehören nicht in den Gerätespeicher).
    verlauf: [],
    // Kennung der Ansage, die gerade erklingt (Original oder Wiederholung).
    spieltId: null,
  };

  /* ── Stimme des Geräts ─────────────────────────────────────────── */
  function geraeteStimmen(lang) {
    try {
      if (!w.speechSynthesis) return [];
      var praefix = lang === "en" ? "en" : "de";
      return w.speechSynthesis.getVoices().filter(function (v) {
        return String(v.lang || "").toLowerCase().indexOf(praefix) === 0;
      });
    } catch (e) { return []; }
  }
  function hatGeraeteStimme() { return geraeteStimmen("de").length > 0; }

  /* ── Ton ───────────────────────────────────────────────────────── */
  function audioKontext() {
    if (!z.ctx) {
      var AC = w.AudioContext || w.webkitAudioContext;
      if (!AC) return null;
      try { z.ctx = new AC(); } catch (e) { return null; }
    }
    return z.ctx;
  }
  function tonPruefen() {
    var ctx = audioKontext();
    z.tonFrei = !!ctx && ctx.state === "running";
    return z.tonFrei;
  }
  function freischalten() {
    var ctx = audioKontext();
    if (ctx && ctx.state !== "running") {
      try { ctx.resume().then(melden, melden); } catch (e) { /* egal */ }
    }
    // Manche Browser geben die Sprachausgabe erst nach einer Äußerung in
    // einer Nutzergeste frei — eine leere tut es.
    try {
      if (w.speechSynthesis && w.SpeechSynthesisUtterance) {
        var u = new w.SpeechSynthesisUtterance(" ");
        u.volume = 0;
        w.speechSynthesis.speak(u);
      }
    } catch (e) { /* egal */ }
  }

  function gongSpielen(art) {
    var ctx = audioKontext();
    if (!ctx || ctx.state !== "running") return Promise.resolve();
    return new Promise(function (fertig) {
      var erledigt = false;
      var ende = function () {
        if (erledigt) return;
        erledigt = true;
        w.setTimeout(fertig, 150);
      };
      try {
        var g = gongPlanen(ctx, art);
        g.letzter.onended = ende;
        w.setTimeout(ende, Math.max(0, (g.endeSec - ctx.currentTime) * 1000) + 250);
      } catch (e) { ende(); }
    });
  }

  function audioLaden(e) {
    var ctx = audioKontext();
    if (!ctx || !e.audio) return Promise.reject(new Error("kein Audio"));
    var abbruch = w.AbortController ? new w.AbortController() : null;
    var ladeFrist = null;
    var geladen = new Promise(function (ok, fehler) {
      ladeFrist = w.setTimeout(function () {
        if (abbruch) abbruch.abort();
        fehler(new Error("Audio-Frist"));
      }, AUDIO_LADE_FRIST_MS);
      w.fetch(base + "ansage/audio/" + e.id, { cache: "no-store", signal: abbruch ? abbruch.signal : undefined })
        .then(function (r) {
          if (!r.ok) throw new Error("HTTP " + r.status);
          return r.arrayBuffer();
        })
        .then(function (buf) {
          return new Promise(function (dok, dfehler) { ctx.decodeAudioData(buf, dok, dfehler); });
        })
        .then(ok, fehler);
    });
    return geladen.then(function (puffer) {
      w.clearTimeout(ladeFrist);
      return puffer;
    }, function (err) {
      w.clearTimeout(ladeFrist);
      throw err;
    });
  }

  function pufferSpielen(puffer) {
    var ctx = audioKontext();
    if (!ctx) return Promise.reject(new Error("kein Ton"));
    return new Promise(function (fertig) {
      var erledigt = false;
      var ende = function () { if (!erledigt) { erledigt = true; fertig(); } };
      var quelle = ctx.createBufferSource();
      quelle.buffer = puffer;
      quelle.connect(ctx.destination);
      quelle.onended = ende;
      w.setTimeout(ende, puffer.duration * 1000 + AUDIO_NACHLAUF_MS);
      quelle.start();
    });
  }

  /* Audio vom Turnier-PC — für eine Wiederholung das schon geladene aus dem
     Verlauf (das auf dem PC nach einer Minute verfällt). Das frisch geladene
     landet am Verlaufseintrag, damit ↻ es später ohne Abruf spielen kann. */
  function audioSpielen(e, v) {
    if (v && v.puffer) return pufferSpielen(v.puffer);
    return audioLaden(e).then(function (puffer) {
      if (v) v.puffer = puffer;
      return pufferSpielen(puffer);
    });
  }

  function segmenteSprechen(e) {
    if (!w.speechSynthesis || !w.SpeechSynthesisUtterance) return Promise.resolve();
    var stimmen = geraeteStimmen(e.lang);
    var kette = Promise.resolve();
    (e.segmente || []).forEach(function (text) {
      kette = kette.then(function () {
        return new Promise(function (fertig) {
          var t = String(text || "").trim();
          if (!t) { fertig(); return; }
          var u = new w.SpeechSynthesisUtterance(t);
          u.lang = e.lang === "en" ? "en-US" : "de-DE";
          u.rate = e.rate || 0.8;
          if (stimmen.length) u.voice = stimmen[0];
          var fertigGemeldet = false;
          var ende = function () { if (!fertigGemeldet) { fertigGemeldet = true; fertig(); } };
          u.onend = ende;
          u.onerror = ende;
          w.setTimeout(ende, 2000 + t.length * 140);
          w.speechSynthesis.speak(u);
        });
      });
    });
    return kette;
  }

  /* ── Warteschlange: strikt nacheinander, nichts überlappt ──────── */
  function naechste() {
    if (z.spielt) return;
    var stueck = z.warteschlange.shift();
    if (!stueck) return;
    var belegung = (z.feed && z.feed.belegung) || [];
    // Eine Wiederholung hat der Nutzer selbst angefordert — sie verfällt
    // nicht und wird nicht gegen die Belegung geprüft.
    if (!stueck.wiederholung && !nochAbspielbar(stueck.e, belegung, stueck.empfangen, jetzt())) {
      naechste();
      return;
    }
    z.spielt = true;
    var e = stueck.e;
    var v = stueck.verlauf || null;
    // Ein Original kommt in den Verlauf, sobald es erklingt — wer nicht hört,
    // liest mit, während gesprochen wird. Eine Wiederholung ist kein neuer
    // Eintrag.
    if (mitVerlauf && !stueck.wiederholung) {
      v = { id: e.id, ms: jetzt(), text: ansageText(e), e: e, puffer: null };
      z.verlauf = verlaufAufnehmen(z.verlauf, v, ANSAGE_VERLAUF_MAX);
    }
    z.spieltId = e.id;
    melden();
    (e.gong ? gongSpielen(e.gong) : Promise.resolve())
      .then(function () {
        return audioSpielen(e, v).catch(function () { return segmenteSprechen(e); });
      })
      .catch(function () { /* weiter mit der nächsten */ })
      .then(function () {
        z.spielt = false;
        z.spieltId = null;
        if (!stueck.wiederholung) {
          z.letzte = { ms: jetzt(), text: String((e.segmente || []).join(" ")).slice(0, 140) };
        }
        melden();
        naechste();
      });
  }

  /* Die Halle, mit der die Seite fragt. Eine leere Liste heißt entweder „nur
     eine Halle" oder „noch kein BTP-Stand" (Turnier-PC startet gerade neu).
     Hat die Seite schon Hallen gesehen, ist es Letzteres: Dann bleibt sie bei
     ihrer Halle — sonst stünde ein Pi nach jedem Neustart des PCs stumm da
     oder hörte kurz fremde Hallen. Nur ohne je gesehene Hallen (Turnier mit
     einer Halle) fragt sie ohne. */
  function wirksameHalle() {
    return z.halls.length || z.hallenGesehen ? z.halle : "";
  }

  /* Darf die Seite mit ihrer Halle spielen und sich melden? Eine fest
     vorgegebene Halle muss es im Turnier geben, sobald es Hallen gibt. */
  function halleOk() {
    if (!halleGueltig(z.halls, z.halle)) return false;
    if (!halleFest || !z.halls.length || String(z.halle).trim() === "") return true;
    return z.halls.some(function (h) { return hallePasst(h, z.halle) && String(h).trim() !== ""; });
  }

  /* Neue Hallenliste übernehmen. Gemerkte oder per Link vorgegebene Halle
     nur übernehmen, wenn es sie gibt; eine leere Liste ändert an der Wahl
     nichts. */
  function hallenUebernehmen(halls) {
    if (JSON.stringify(halls) === JSON.stringify(z.halls)) return;
    z.halls = halls;
    if (halls.length) {
      z.hallenGesehen = true;
      var passend = halls.filter(function (h) { return hallePasst(h, z.halle) && z.halle !== ""; })[0];
      z.halle = passend || (halleFest ? z.halle : "");
    }
    hallenMelden(halls);
  }

  /* Spielt eine Ansage aus dem Verlauf noch einmal — nur auf diesem Gerät,
     hinten in derselben strikten Schlange (nie über eine laufende Ansage).
     Der Tipp auf ↻ ist eine Nutzergeste: Er gibt den Ton frei, falls nötig —
     und die Schlange läuft erst weiter, wenn der Ton wieder läuft, sonst
     fiele der Gong weg (Kontext nach Anruf/Bildschirm aus angehalten).
     Mehrfaches Tippen stapelt nichts: Liegt dieselbe Ansage schon als
     Wiederholung in der Schlange, bleibt es bei der einen. */
  function wiederholen(id) {
    var auftrag = wiederholungsAuftrag(z.verlauf, id);
    if (!auftrag) return false;
    var schonDa = z.warteschlange.some(function (s) { return s.wiederholung && s.e.id === id; });
    if (schonDa) return true;
    z.warteschlange.push(auftrag);
    var ctx = audioKontext();
    if (ctx && ctx.state !== "running") {
      freischalten();
      var weiter = function () { naechste(); };
      try { ctx.resume().then(weiter, weiter); } catch (e) { weiter(); }
      return true;
    }
    naechste();
    return true;
  }

  function halleWaehlen(halle) {
    var vorher = z.halle;
    z.halle = String(halle || "");
    // Neue Halle: Was schon in der Schlange liegt, gehört zur alten — bis auf
    // Wiederholungen, die der Nutzer selbst angefordert hat. Der Verlauf
    // bleibt stehen: Er ist das, was dieses Gerät gespielt hat.
    if (vorher !== z.halle) {
      z.warteschlange = z.warteschlange.filter(function (s) { return s.wiederholung; });
    }
    melden();
  }

  /* ── Abfrage ───────────────────────────────────────────────────── */
  function abfragen() {
    if (z.abrufLaeuft) return;
    if (z.fehltSeit && jetzt() - z.fehltSeit < ANSAGE_FEHLT_PAUSE_MS) return;
    var hoert = meldetPraesenz(z.feed, hatGeraeteStimme(), tonPruefen()) && halleOk();
    var abbruch = w.AbortController ? new w.AbortController() : null;
    var abruf;
    // Ein synchroner Wurf darf die Sperre nie setzen, ohne sie zu lösen —
    // sonst fragte die Seite nie wieder (Review E1b).
    try {
      abruf = w.fetch(feedAdresse(base, wirksameHalle(), z.stand.lastId, hoert),
        { cache: "no-store", signal: abbruch ? abbruch.signal : undefined });
    } catch (e) {
      abruf = Promise.reject(e);
    }
    var frist = w.setTimeout(function () { if (abbruch) abbruch.abort(); }, ANSAGE_FEED_FRIST_MS);
    z.abrufLaeuft = true;
    abruf
      .then(function (r) {
        if (r.status === 404) {
          z.fehltSeit = jetzt();
          throw new Error("kein Feed");
        }
        if (!r.ok) throw new Error("HTTP " + r.status);
        return r.json();
      })
      .then(function (feed) {
        z.verbunden = true;
        z.fehltSeit = 0;
        z.fehlversuche = 0;
        z.feed = feed;
        hallenUebernehmen(feed.halls || []);
        var r = neueEintraege(z.stand, feed);
        z.stand = r.stand;
        // Nur abspielen, wenn diese Seite wirklich bereit ist — sonst liefen
        // Aufrufe einer fremden Halle oder ohne Ton in die Schlange.
        if (z.tonFrei && halleOk()) {
          var empfangen = jetzt();
          r.neu.forEach(function (e) { z.warteschlange.push({ e: e, empfangen: empfangen }); });
          naechste();
        }
      })
      .catch(function () { z.verbunden = false; z.fehlversuche += 1; })
      .then(function () {
        w.clearTimeout(frist);
        z.abrufLaeuft = false;
        melden();
      });
  }

  function starten() {
    // Läuft der Ton schon (Pi mit Autoplay-Freigabe, Kiosk-App), ist die
    // Seite ohne Tippen bereit.
    audioKontext();
    melden();
    abfragen();
    w.setInterval(abfragen, ANSAGE_TAKT_MS);
  }

  return {
    zustand: z,
    starten: starten,
    abfragen: abfragen,
    freischalten: freischalten,
    halleWaehlen: halleWaehlen,
    wiederholen: wiederholen,
    halleOk: halleOk,
    tonPruefen: tonPruefen,
    hatGeraeteStimme: hatGeraeteStimme,
    stimmeArt: function () { return stimme(z.feed && z.feed.hostAudio, hatGeraeteStimme()); },
  };
}

/* ── Ansage-Modus der Monitor-Seiten (E1b) ───────────────────────────── */

/** Liest den Modus aus der Adresse: `?ansage=<Halle>` schaltet ihn ein,
 *  auch mit leerem Wert (Turnier mit einer Halle). */
export function ansageModusAusQuery(search) {
  var such;
  try { such = new URLSearchParams(search || ""); } catch (e) { return null; }
  if (!such.has("ansage")) return null;
  return { halle: String(such.get("ansage") || "").trim() };
}

/** Den Ansage-Feed gibt es in E1b nur am Turnier-PC im Hallennetz (Basis
 *  `/`); über das Relay (`/bts-relay/<ns>/`) kommt er erst mit E4. */
export function ansageImLan(base) {
  return base === "/";
}

/**
 * Was der Monitor im Ansage-Modus als kleinen Hinweis zeigt — leer, wenn
 * alles läuft (dann bleibt die Anzeige unverändert). `lage`: `{ lan, fehlt,
 * verbunden, feed, tonFrei, halleOk, halle, stimme }`.
 */
export function monitorHinweis(lage) {
  if (!lage.lan) return "Ansage: nur im Hallennetz (LAN) am Turnier-PC — über die Cloud noch nicht.";
  if (lage.fehlt) return "Ansage: Der Turnier-PC bietet hier keine Ansagen an (bts-light zu alt oder Slave-Brücke).";
  if (!lage.verbunden || !lage.feed) return "Ansage: keine Verbindung zum Turnier-PC.";
  if (!lage.feed.aktiv) return "Ansage: " + grundText(lage.feed.grund);
  if (!lage.halleOk) {
    return lage.halle
      ? "Ansage: Die Halle „" + lage.halle + "“ gibt es in diesem Turnier nicht."
      : "Ansage: Bitte die Halle angeben (?ansage=<Halle>).";
  }
  if (!lage.tonFrei) return "Ansage: Ton gesperrt — der Browser braucht die Autoplay-Freigabe (Pi-Einrichtung aktualisieren).";
  if (lage.stimme === "keine") return "Ansage: Dieses Gerät kann nicht sprechen (kein Audio vom Turnier-PC, keine Gerätestimme).";
  return "";
}

/**
 * Startet den Ansage-Modus einer Monitor-Seite, wenn die Adresse ihn
 * verlangt. Die Anzeige selbst bleibt unberührt; nur bei einem Problem
 * erscheint unten links ein kleiner Hinweis. Ohne freigegebenen Ton meldet
 * sich die Seite nicht als Ansage-Gerät (das regelt der Kern).
 */
export function ansageMonitorStarten(base, welt, dokument) {
  var modus = ansageModusAusQuery(welt.location && welt.location.search);
  if (!modus) return null;
  var hinweis = dokument.createElement("div");
  hinweis.id = "ansage-hinweis";
  hinweis.setAttribute("role", "status");
  hinweis.style.cssText = "position:fixed;left:.6vmin;bottom:.6vmin;z-index:2147483000;" +
    "max-width:60vw;padding:.4vmin .9vmin;border-radius:.6vmin;font:1.6vmin/1.3 system-ui,sans-serif;" +
    "background:rgba(15,23,42,.82);color:#fbbf24;pointer-events:none;display:none";
  dokument.body.appendChild(hinweis);
  function zeigen(text) {
    hinweis.textContent = text;
    hinweis.style.display = text ? "block" : "none";
  }
  if (!ansageImLan(base)) {
    // Kein Abruf, keine Fehlerschleife: Im Cloud-Pfad gibt es den Feed noch nicht.
    zeigen(monitorHinweis({ lan: false }));
    return null;
  }
  var kern = ansageKern({
    base: base,
    halle: modus.halle,
    halleFest: true,
    welt: welt,
    beiAenderung: function () {
      var z = kern ? kern.zustand : null;
      if (!z) return;
      zeigen(monitorHinweis({
        lan: true,
        fehlt: !!z.fehltSeit,
        // Erst nach drei Fehlschlägen in Folge (≈ 5 s) — ein WLAN-Wackler
        // soll nicht jedes Mal einen Hinweis in die Halle blitzen lassen.
        verbunden: z.verbunden || (!!z.feed && z.fehlversuche < 3),
        feed: z.feed,
        tonFrei: kern.tonPruefen(),
        halleOk: kern.halleOk(),
        halle: z.halle,
        stimme: kern.stimmeArt(),
      }));
    },
  });
  // Kein Freischalt-Knopf am TV — aber eine zufällige Geste (Maus, Taste)
  // gibt den Ton frei, falls die Autoplay-Freigabe fehlt.
  ["pointerdown", "keydown"].forEach(function (art) {
    dokument.addEventListener(art, kern.freischalten, { passive: true });
  });
  kern.starten();
  return kern;
}
