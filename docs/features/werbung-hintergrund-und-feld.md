# Werbung: Hintergrundfarbe je Bild + Feldbezeichnung — Spezifikation

> Status: **abgestimmt 2026-08-20** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Nutzer-Idee vom 20.08.2026. Betroffene Crates: `src-tauri`, `relay`, `relay-proto`, `src`.
> ADR: [0041](../adr/0041-werbe-stil-je-bild.md).

## Kontext / Problem

Ein Court-Monitor zeigt im Leerlauf — solange kein Spiel auf seinem Feld läuft
— die hinterlegten **Werbebilder** im Vollbild (`monitor.html`, `#ad-view`).
Das Bild wird mit `object-fit: contain` eingepasst; was es nicht abdeckt,
bleibt **schwarz**, hart verdrahtet.

Daraus folgen zwei Ärgernisse für den Turnierleiter:

1. **Sponsorlogos sind für ihren eigenen Hintergrund gemacht.** Ein Logo mit
   weißem Rand oder transparentem Grund steht auf Schwarz im Kasten oder
   verliert seine Kontur. Der Sponsor sieht auf dem Hallenbildschirm anders aus
   als auf seinem Banner.
2. **Im Leerlauf verschwindet die Feldnummer.** Zeigt der Monitor Werbung,
   sieht niemand mehr, welches Feld das ist — die große Feldnummer erscheint
   heute nur, wenn **gar kein** Werbebild hinterlegt ist (`#ad-fallback`,
   `.court:empty`). Gerade morgens vor dem ersten Aufruf steht die Halle voller
   Bildschirme, die nicht sagen, wo man ist.

## Zielbild & Erfolgskriterien

Der Turnierleiter stellt **je Bild** ein: eine Hintergrundfarbe und, ob die
Feldbezeichnung mit erscheint. Ohne Zutun bleibt alles wie bisher.

- Beim nächsten Turnier zeigen die Leerlauf-Monitore die eingestellte Farbe und
  — wo gewünscht — die Feldbezeichnung, ohne dass während des Turniers
  nachjustiert werden muss.
- Ein Turnier, das nichts einstellt, sieht **exakt** aus wie vor dem Update.
- Die Einstellung ist ohne Erklärung bedienbar: ein Farbfeld und ein Häkchen in
  der Zeile des Bildes, mit Vorschau daneben.
- Es gibt keinen Weg, die Feldbezeichnung unlesbar zu konfigurieren.

## Nicht-Ziele

- Die **kleine Leisten-Werbung** neben dem Turnierlogo (`in_bar`) — unverändert.
- Der **Leerlauf-Fallback ohne Bild** (große Feldnummer) — unverändert.
- `combo.html`, `overview.html`, `preparation.html`, `tablet.html`, `tv.html`,
  `lobby.html`, `winners.html` — sie zeigen keine Vollbild-Werbung und werden
  bewusst nicht nachgezogen.
- **badhub**: Farbe und Feld-Häkchen reisen nicht zum Check-In-Branding.
- Rotationstakt, Bild-Upload und -Verwaltung.
- `MonitorTarget::AdSingle` im Cloud (heute nicht unterstützt) und die
  Index-Verschiebung der Cloud-Ad-Auflösung — eigene Aufgaben, siehe ADR 0041.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** `src-tauri/src/tablet/monitor.rs` (Store + Kontrast),
  `commands.rs` (`set_court_ad_style`, `CourtAd`, Aufräumen),
  `tablet/server.rs` (Cache, `/info/ad/state`, `MonitorState`),
  `tablet/relay_client.rs` (Fingerabdruck, Upload), `relay-proto` (`AdUpload`,
  `MonitorState`), `relay/` (Durchreichen), `assets/monitor.html`,
  `assets/ad.html`, `src/pages/SetupWizard.tsx`, `src/api.ts`, `src/types.ts`.
- **R1:** Die Einstellung geht ausschließlich über den Tauri-Command
  `set_court_ad_style`; das Frontend fasst keine Datei an.
- **R3:** Beide Wege tragen den Stil — LAN über `/info/ad/state` und
  `MonitorState`, Cloud über `AdUpload` → `MonitorBundle` → dieselben Routen am
  Relay. Im **LanAndCloud**-Parallelbetrieb kann eine Farbänderung bis zu 30 s
  (`MONITOR_TICK`) brauchen, bis Cloud-Monitore sie zeigen, während LAN-Monitore
  sie beim nächsten Poll haben. Das ist zugelassen und unten als Kriterium
  festgehalten.
- **Konfiguration & Abwärtskompatibilität:** **Keine** neuen Felder in
  `config.rs`. Der Stil liegt als **dritte** Seiten-Datei `court-ad-style.json`
  im `court-ads/`-Verzeichnis, Muster `court-ad-bar.json`. Bewusst **nicht** in
  `court-ad-labels.json` mit hinein: dessen Deserializer ist strikt
  (`HashMap<String,String>`) und schluckt Fehler mit `unwrap_or_default()` — ein
  Formatwechsel löschte beim Auto-Update still **alle Anzeigenamen**, in beide
  Richtungen (auch beim Rollback). Fehlende Datei = Schwarz, Feld aus.
  `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:** Keine personenbezogenen Daten. Die Feldbezeichnung stammt aus
  BTP und steht ohnehin an jedem Feld.
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency. `ist_hex_farbe` aus
  `hall_colors.rs` wird wiederverwendet (auf `pub(crate)` gehoben), die
  Kontrastrechnung kommt neu dazu — im Repo gibt es bisher keine.

## Verhalten im Detail

**Hintergrundfarbe.** Je Bild ein Hex-Wert `#rrggbb`. Er füllt die Fläche des
Leerlauf-Vollbilds; das Bild selbst bleibt `object-fit: contain`. Standard
`#000000` — der heutige Zustand.

**Feldbezeichnung.** Je Bild ein Häkchen. Ist es gesetzt, wird das Bild so weit
verkleinert, dass ringsum ein Rand in der Hintergrundfarbe bleibt, und die
Feldbezeichnung steht **oben links** in diesem Rand — also auf der reinen
Farbe, nie auf dem Motiv. Ohne Häkchen nutzt das Bild die volle Fläche wie
heute.

**Rotation.** Trägt **mindestens ein** Bild des Feldes das Häkchen, bleibt die
Feldbezeichnung während der ganzen Leerlauf-Rotation stehen und **alle** Bilder
werden gleich verkleinert. Sonst spränge die Bildgröße im Rotationstakt
(Standard 10 s) und die Bezeichnung flackerte.

**Schriftfarbe.** Automatisch aus der Hintergrundfarbe: relative Luminanz nach
WCAG, heller Grund → dunkle Schrift, dunkler Grund → helle. Nicht einstellbar.

**Leere Feldbezeichnung.** Liefert der Host keine (im Cloud füllt der Relay
`court_labels` nur aus `MatchAssigned`/`MatchCleared` — morgens vor dem ersten
Aufruf also leer), entfällt die Bezeichnung **und** die Verkleinerung: Das Bild
läuft in voller Fläche, statt einen leeren Rand zu zeigen.

**Übergang.** Wechselt die Rotation zwischen Bildern mit verschiedenen Farben,
blenden Bild und Farbe weich ineinander (Muster: `ad.html` macht das schon),
damit der Fernseher nicht im 10-Sekunden-Takt zwischen Weiß und Schwarz blitzt.

**Zweite Vollbild-Werbefläche.** `ad.html` (reine Werbe-Anzeige auf
Info-Bildschirmen) übernimmt die **Farbe**, damit dasselbe Motiv in der Halle
überall gleich aussieht. Eine Feldbezeichnung gibt es dort nicht — die Seite
hat keinen Feldbezug, das Häkchen ist dort wirkungslos.

## Akzeptanzkriterien

- [ ] Ohne jede Einstellung zeigt ein Leerlauf-Monitor Werbung auf Schwarz,
      ohne Feldbezeichnung — wie vor dem Update.
- [ ] Eine je Bild gesetzte Farbe erscheint als Hintergrund des Leerlauf-Vollbilds,
      im LAN spätestens beim nächsten Poll, im Cloud binnen 30 s.
- [ ] Mit gesetztem Häkchen steht die Feldbezeichnung oben links auf der
      Hintergrundfarbe, das Bild ist so verkleinert, dass sie nie auf dem Motiv liegt.
- [ ] Die Schriftfarbe kontrastiert automatisch: heller Grund → dunkle Schrift,
      dunkler Grund → helle.
- [ ] Tragen von drei rotierenden Bildern nur eines das Häkchen, bleibt die
      Bezeichnung über alle drei stehen und die Bildgröße springt nicht.
- [ ] Ist die Feldbezeichnung leer, erscheint kein leerer Rand — das Bild läuft
      in voller Fläche.
- [ ] Der Command lehnt alles ab, was nicht `#rrggbb` in Kleinbuchstaben ist;
      jede Anzeigeseite prüft den Wert erneut, bevor er in ein `style`-Attribut
      geht, und fällt sonst auf Schwarz zurück.
- [ ] Wird ein Bild gelöscht, verschwindet sein Stil-Eintrag mit.
- [ ] Das Setzen eines Stils löst **keinen** badhub-Push aus (anders als das
      „Leiste"-Häkchen).
- [ ] Eine reine Farbänderung führt im Cloud zu einem neuen Upload (der
      Fingerabdruck ändert sich) und im LAN zu einem Neuzeichnen (der Stil steckt
      im `applyAds`-Schlüssel).
- [ ] Alter Relay + neuer Host: Die Felder werden verworfen, Monitore zeigen
      Schwarz — kein Fehlbild. Neuer Relay + alter Host: Defaults greifen.
- [ ] Rollback auf die Vorversion: Labels und „Leiste"-Häkchen bleiben
      unversehrt, die Stil-Datei wird ignoriert.
- [ ] `ad.html` zeigt dieselbe Farbe, aber nie eine Feldbezeichnung.

## Tests

**Rust (`cargo test`)**
- `read_write_ad_style_roundtrip` — Schreiben/Lesen, unbekannte Datei → Default,
  leerer Store löscht die Datei (Muster `read_write_ad_bar_roundtrip`).
- `schriftfarbe_kontrastiert_zum_grund` — Schwarz → hell, Weiß → dunkel, dazu
  je ein mittelheller Grund beidseits der Schwelle.
- `ungueltige_farbe_wird_abgelehnt` — Command weist `rot`, `#FFF`, `#RRGGBB`
  (Großbuchstaben) und Leerstring ab.
- `stil_wird_beim_loeschen_aufgeraeumt` — `remove_court_ad` entfernt den Eintrag.
- `ad_upload_style_defaults_to_black_and_off` — altes Frame ohne die neuen
  Felder (Muster `ad_upload_in_bar_and_logo_default_to_off`).
- `monitor_fingerprint_reagiert_auf_stiländerung`.

**Frontend:** `npm run build` fehlerfrei; `scripts/check-asset-syntax.mjs` grün.

**Manuell am Turnier:** ein Bild mit weißem Grund + Häkchen auf einem Feld,
zweites Bild schwarz ohne Häkchen — Rotation beobachten (kein Springen, kein
Blitzen), einmal im LAN- und einmal im Cloud-Modus.

## Risiken & Rollback

- **Im laufenden Turnier** ist die Änderung rein kosmetisch: Sie berührt weder
  Ergebnisse noch Feldvergabe. Schlimmstenfalls sieht ein Monitor falsch aus.
- **Auslieferungs-Zwischenstufe:** Der Relay deployt bei jedem main-Merge
  automatisch, die App erst mit dem Release-Tag. Zwischen beidem läuft ein neuer
  Relay mit alten Hosts — die Defaults greifen, es bleibt schwarz.
- **Rollback** ist gefahrlos: Die ältere Version kennt `court-ad-style.json`
  nicht und ignoriert sie; Labels und „Leiste"-Häkchen liegen in eigenen Dateien
  und bleiben unberührt. Genau dafür ist der Stil ein dritter Store.

## Offene Fragen / Annahmen

Vom Nutzer entschieden: nur Vollbild-Leerlauf · Feldbezeichnung klein in einer
Ecke · Häkchen **je Bild** · Schriftfarbe automatisch · Standard Schwarz ·
Bild wird verkleinert, damit ein farbiger Rand bleibt.

Als Annahme umgesetzt (im Review kippbar): Feldbezeichnung bleibt über die
ganze Rotation stehen, sobald ein Bild sie will · sanftes Überblenden ·
`ad.html` bekommt Farbe, aber kein Feld · freier Farbwähler statt Palette ·
Mini-Vorschau je Bildzeile.

Bewusst offen: Ob die Feldbezeichnung die **Hallen-Farbmarke** der Match-Ansicht
trägt — sie tut es vorerst nicht, der nackte BTP-Label-Text genügt und hält die
Ecke ruhig.
