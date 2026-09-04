//! Aushang mit QR-Codes: ein A4-Blatt, das die Turnierleitung ausdruckt und
//! in die Halle hängt. Es führt Spielerinnen, Spieler und Zuschauer auf die
//! beiden öffentlichen badhub-Seiten des Turniers:
//!
//! * **Teilnehmerliste** → von dort auf das persönliche Spielerprofil mit
//!   Halle, Restspielen und Zeitprognose,
//! * **Liveticker** → alle Felder in Echtzeit.
//!
//! Beide Adressen leitet das Modul aus **einer** Angabe ab: der öffentlichen
//! Live-Seite aus den Einstellungen (`BadhubConfig::live_url`, z. B.
//! `https://badhub.de/live?t=bvbb`). Ohne sie gibt es keinen Aushang: Das
//! Kürzel darin ist der **Verband** (`bvbb` = Berlin-Brandenburg, siehe die
//! Vorlagen in `src/presets.ts`), und ein geratenes führte die Halle auf die
//! Live-Seite eines fremden Verbandes.
//!
//! Das Dokument ist wie der Schiedsrichterzettel **skriptfrei** (ADR 0039):
//! Der Kern liefert fertiges HTML, das Frontend zeigt es in einem
//! `iframe srcdoc` und druckt über den WebView.
//!
//! Doku: `docs/aushang.md`.

use relay_proto::html_escape;

/// Längste akzeptierte Kürzel-Länge. Großzügig, aber begrenzt: Das Kürzel
/// wandert in eine URL und in den QR-Code.
const MAX_KUERZEL: usize = 40;

/// Alles, was auf das Blatt kommt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AushangDaten {
    /// Turniername aus BTP. Leer ⇒ Kopfzeile trägt nur die badhub-Marke.
    pub turnier: String,
    /// Turnierlogo als `data:`-URI (bereits geprüft von
    /// [`crate::tablet::scoresheet::logo_data_uri`]). `None` ⇒ kein Logo.
    pub logo: Option<String>,
    /// Liveticker, aus Basis + Kürzel neu gebaut (nicht die eingetragene
    /// Adresse — die kann Parameter oder einen anderen Unterpfad tragen).
    pub url_ticker: String,
    /// Teilnehmerliste, aus Basis + Kürzel abgeleitet.
    pub url_teilnehmer: String,
}

/// Prüft ein Verbandskürzel: nur die Zeichen, die badhub in seinen Live-URLs
/// verwendet. Alles andere wäre entweder ein Tippfehler oder ein Versuch,
/// über die Konfiguration eine fremde Adresse in den QR-Code zu schmuggeln.
fn ist_kuerzel(wert: &str) -> bool {
    !wert.is_empty()
        && wert.len() <= MAX_KUERZEL
        && wert
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Verbandskürzel aus der öffentlichen Live-Seite.
///
/// Akzeptiert beide Schreibweisen, die auf badhub vorkommen:
/// `…/live?t=<kürzel>` (die dokumentierte) und `…/live/<kürzel>[/…]`.
pub fn kuerzel_aus_live_url(live_url: &str) -> Option<String> {
    let s = live_url.trim();
    if s.is_empty() {
        return None;
    }
    let (pfad, query) = match s.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (s, None),
    };

    // 1. `?t=<kürzel>` – die dokumentierte Form.
    if let Some(q) = query {
        for paar in q.split('&') {
            if let Some(wert) = paar.strip_prefix("t=") {
                let wert = wert.split('#').next().unwrap_or(wert);
                if ist_kuerzel(wert) {
                    return Some(wert.to_string());
                }
                // Ein `t`, das kein sauberes Kürzel ist, ist ein Fehler in der
                // Eingabe — dann lieber gar kein Aushang als ein falscher.
                return None;
            }
        }
    }

    // 2. `…/live/<kürzel>` – die Kurzform aus der Adresszeile.
    let pfad = pfad.split('#').next().unwrap_or(pfad);
    let mut segmente = pfad.split('/').filter(|s| !s.is_empty());
    while let Some(seg) = segmente.next() {
        if seg.eq_ignore_ascii_case("live") {
            let kandidat = segmente.next()?;
            return ist_kuerzel(kandidat).then(|| kandidat.to_string());
        }
    }
    None
}

/// Schema + Host der Live-Seite, damit die Teilnehmerliste auf **derselben**
/// Installation landet (Testaufbauten laufen nicht auf badhub.de).
fn basis_aus_live_url(live_url: &str) -> Option<String> {
    let s = live_url.trim();
    let (schema, rest) = match s.strip_prefix("https://") {
        Some(r) => ("https://", r),
        None => ("http://", s.strip_prefix("http://")?),
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_ok = !host.is_empty()
        && host.len() <= 255
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    host_ok.then(|| format!("{schema}{host}"))
}

/// Hängt die Turnier-GUID als `g=` an eine Live-Adresse (ADR 0054): Der
/// Verbandsschlüssel `t` führt bei mehreren laufenden Turnieren auf eine
/// Auswahl, `g` direkt auf dieses Turnier. Ohne GUID bleibt die Adresse,
/// wie sie ist — badhub zeigt dann wie früher den Verbandsschlüssel.
pub fn link_mit_guid(url: &str, guid: Option<&str>) -> String {
    let url = url.trim();
    match guid {
        Some(g) if !url.is_empty() => {
            let trenner = if url.contains('?') { '&' } else { '?' };
            format!("{url}{trenner}g={g}")
        }
        _ => url.to_string(),
    }
}

/// Stellt die Daten für das Blatt zusammen. `None`, wenn die öffentliche
/// Live-Seite fehlt oder nicht auswertbar ist.
pub fn daten_aus(
    live_url: &str,
    turnier: &str,
    logo: Option<String>,
    guid: Option<&str>,
) -> Option<AushangDaten> {
    let kuerzel = kuerzel_aus_live_url(live_url)?;
    let basis = basis_aus_live_url(live_url)?;
    // **Beide** Adressen werden neu gebaut, keine wird durchgereicht. Sonst
    // landet auf dem Blatt, was in den Einstellungen steht: die
    // Teilnehmerliste (die dieses Modul als Live-Seite akzeptiert) stünde
    // dann unter der Überschrift „Liveticker", und angehängte Parameter wie
    // `&display=monitor` schickten die halbe Halle in die Monitor-Ansicht.
    Some(AushangDaten {
        turnier: turnier.trim().to_string(),
        logo,
        url_ticker: link_mit_guid(&format!("{basis}/live?t={kuerzel}"), guid),
        url_teilnehmer: format!("{basis}/live/{kuerzel}/teilnehmer"),
    })
}

/// Kurzform einer Adresse für den Klartext unter dem QR-Code: ohne Schema
/// und `www.`, damit sie auch bei langen Kürzeln in die Karte passt.
fn anzeige_url(url: &str) -> String {
    let ohne_schema = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    ohne_schema.trim_start_matches("www.").to_string()
}

/// QR-Code als SVG-Schnipsel zum Einbetten in HTML.
///
/// Fehlerkorrektur **H**: Der Aushang hängt tagelang in der Halle, wird blass
/// kopiert und geknickt — der Code muss das aushalten. Ohne Ruhezone, die
/// setzt das Blatt selbst als Weißraum um den Code.
pub fn qr_svg(text: &str) -> Result<String, String> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::H)
        .map_err(|e| e.to_string())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .quiet_zone(false)
        .dark_color(qrcode::render::svg::Color("#0f172a"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    // Der Renderer stellt eine XML-Deklaration voran; inline in HTML hat die
    // nichts zu suchen.
    Ok(match svg.find("<svg") {
        Some(pos) => svg[pos..].to_string(),
        None => svg,
    })
}

/// Eine der beiden Karten auf dem Blatt.
struct Karte<'a> {
    akzent: &'a str,
    kicker: &'a str,
    titel: &'a str,
    /// Fest verdrahteter Werbetext; enthält bewusst `<strong>`-Auszeichnung
    /// und stammt nie aus Nutzereingaben.
    text: &'a str,
    url: &'a str,
}

fn karte_html(k: &Karte<'_>) -> Result<String, String> {
    Ok(format!(
        r#"      <section class="karte" style="--akzent:{akzent}">
        <p class="kicker">{kicker}</p>
        <h2>{titel}</h2>
        <p class="text">{text}</p>
        <div class="qr-feld"><div class="qr">{qr}</div></div>
        <p class="url">{anzeige}</p>
      </section>"#,
        akzent = k.akzent,
        kicker = k.kicker,
        titel = k.titel,
        text = k.text,
        qr = qr_svg(k.url)?,
        anzeige = html_escape(&anzeige_url(k.url)),
    ))
}

/// Baut das druckfertige A4-Blatt.
pub fn render_html(d: &AushangDaten) -> Result<String, String> {
    let kopf_logo = match d.logo.as_deref() {
        // Das Logo kommt als geprüfte `data:`-URI; escapen schützt trotzdem
        // den Attribut-Kontext.
        Some(uri) => format!(r#"<img class="logo" src="{}" alt="">"#, html_escape(uri)),
        None => r#"<span class="punkt"></span>"#.to_string(),
    };
    let kopf_turnier = if d.turnier.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="turnier">{}</p>"#, html_escape(&d.turnier))
    };

    let links = karte_html(&Karte {
        akzent: "#15803d",
        kicker: "Für Spielerinnen und Spieler",
        titel: "Dein Spielerprofil",
        text: "Tipp in der Teilnehmerliste auf deinen Namen &mdash; und du hast deinen ganzen \
               Turniertag vor dir: <strong>in welcher Halle du spielst</strong>, sobald es \
               feststeht, <strong>wie viele Spiele noch vor dir liegen</strong> und \
               <strong>wann dein nächstes Spiel voraussichtlich dran ist</strong>. Dazu deine \
               Ergebnisse &mdash; und ein Link, den du Eltern, Trainerin und Verein schicken \
               kannst.",
        url: &d.url_teilnehmer,
    })?;
    let rechts = karte_html(&Karte {
        akzent: "#b45309",
        kicker: "Für alle, die mitfiebern",
        titel: "Der Liveticker",
        text: "Alle Felder auf einen Blick, <strong>Punkt für Punkt in Echtzeit</strong>. Du \
               siehst, welches Spiel gerade läuft und wie es bei deinen Vereinskameraden steht. \
               Wer heute nicht in die Halle kommen konnte, sitzt daheim trotzdem in der ersten \
               Reihe &mdash; einmal öffnen, der Stand läuft von selbst mit.",
        url: &d.url_ticker,
    })?;

    Ok(format!(
        r#"<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<title>Aushang – badhub Liveticker</title>
<style>
  /* A4 hoch, randlos angesteuert. `print-color-adjust: exact` ist Pflicht:
     Ohne den Schalter druckt der WebView Flächen nicht mit (Erfahrung aus
     dem Zettel-Druck, v0.9.250). Das Blatt ist zusätzlich so gebaut, dass es
     auch ganz ohne Hintergrundfarben lesbar bleibt: dunkle Schrift auf Weiß,
     Farbe nur in Rahmen und Text. */
  @page {{ size: A4 portrait; margin: 0; }}
  * {{ box-sizing: border-box; margin: 0; padding: 0;
      -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
  :root {{ --tinte: #0f172a; --grau: #475569; --linie: #cbd5e1; }}
  html, body {{ background: #e2e8f0; }}
  body {{ font-family: "Segoe UI", "Inter", system-ui, -apple-system, sans-serif;
         color: var(--tinte); display: flex; justify-content: center;
         align-items: flex-start; padding: 12mm 0; }}
  .blatt {{ width: 210mm; height: 297mm; background: #fff; padding: 14mm 14mm 12mm;
           display: flex; flex-direction: column; overflow: hidden;
           box-shadow: 0 8px 30px rgba(15,23,42,.25); }}

  .kopf {{ display: flex; align-items: center; gap: 4mm; }}
  .kopf .logo {{ height: 14mm; width: auto; max-width: 50mm; object-fit: contain; }}
  .kopf .punkt {{ width: 3mm; height: 3mm; border-radius: 50%; background: #f59e0b; }}
  .kopf .marke {{ flex: none; white-space: nowrap; font-size: 9.5pt; font-weight: 700;
                 letter-spacing: .18em; text-transform: uppercase; color: var(--grau); }}
  /* Klein genug, dass auch ein zweizeiliger Turniername INNERHALB der
     Logohöhe bleibt — sonst wächst der Kopf und das Blatt läuft unten über.
     Der Deckel auf zwei Zeilen ist kein Schönheitsmaß: BTP-Turniernamen
     werden lang („Offene Bezirksmeisterschaften der Altersklassen …“), und
     ohne ihn schiebt der Kopf die Schluss-Zeile aus dem Blatt — unsichtbar,
     weil `.blatt` abschneidet. Lieber ein gekürzter Name als ein
     gekürztes Blatt. */
  .kopf .turnier {{ margin-left: auto; text-align: right; font-size: 11.5pt;
                   font-weight: 700; line-height: 1.25; max-width: 80mm;
                   display: -webkit-box; -webkit-line-clamp: 2;
                   -webkit-box-orient: vertical; overflow: hidden;
                   text-overflow: ellipsis; }}

  h1 {{ margin-top: 5mm; font-size: 40pt; line-height: 1.02; font-weight: 800;
       letter-spacing: -.02em; }}
  h1 .hell {{ color: #94a3b8; }}
  .subline {{ margin-top: 5mm; max-width: 165mm; font-size: 13pt; line-height: 1.45;
             color: var(--grau); }}
  .subline strong {{ color: var(--tinte); font-weight: 700; }}

  .karten {{ display: flex; gap: 8mm; margin-top: 7mm; }}
  .karte {{ flex: 1 1 0; min-width: 0; border: .8mm solid var(--akzent);
           border-radius: 4mm; padding: 7mm 7mm 6mm; display: flex;
           flex-direction: column; }}
  .kicker {{ font-size: 8.5pt; font-weight: 700; letter-spacing: .12em;
            text-transform: uppercase; color: var(--akzent); }}
  .karte h2 {{ margin-top: 2mm; font-size: 21pt; font-weight: 800; line-height: 1.1; }}
  .karte .text {{ margin-top: 3.5mm; font-size: 10.5pt; line-height: 1.45;
                 color: var(--grau); min-height: 32mm; }}
  .karte .text strong {{ color: var(--tinte); font-weight: 700; }}
  .qr-feld {{ margin-top: auto; padding-top: 4mm; display: flex; justify-content: center; }}
  .qr {{ width: 66mm; padding: 4mm; background: #fff; border: .4mm solid var(--linie);
        border-radius: 2mm; }}
  .qr svg {{ display: block; width: 100%; height: auto; }}
  .url {{ margin-top: 4mm; text-align: center; font-size: 10pt; font-weight: 700;
         color: var(--akzent); word-break: break-all; }}

  .anleitung {{ margin-top: 7mm; padding-top: 5mm; border-top: .4mm solid var(--linie);
               display: flex; align-items: center; justify-content: space-between; gap: 2mm; }}
  .schritt {{ display: flex; align-items: center; gap: 2.5mm; font-size: 10.5pt;
             white-space: nowrap; }}
  .schritt .nr {{ width: 6.5mm; height: 6.5mm; flex: none; border-radius: 50%;
                 background: var(--tinte); color: #fff; font-size: 9pt; font-weight: 700;
                 display: flex; align-items: center; justify-content: center; }}
  .pfeil {{ color: var(--linie); font-size: 12pt; }}
  .schluss {{ margin-top: 5mm; display: flex; align-items: baseline;
             justify-content: space-between; gap: 6mm; }}
  .schluss .ruf {{ font-size: 14pt; font-weight: 800; }}
  .schluss .ruf em {{ font-style: normal; color: #b45309; }}
  .schluss .klein {{ flex: none; font-size: 9.5pt; color: var(--grau); text-align: right; }}

  @media print {{
    html, body {{ background: #fff; }}
    body {{ padding: 0; display: block; }}
    .blatt {{ box-shadow: none; }}
  }}
</style>
</head>
<body>
  <div class="blatt">
    <div class="kopf">
      {kopf_logo}
      <span class="marke">badhub.de &middot; Liveticker</span>
      {kopf_turnier}
    </div>

    <h1>Zwei Codes &mdash;<br>und du bist <span class="hell">mittendrin.</span></h1>

    <p class="subline">
      Spielplan, Ergebnisse und jeder Ballwechsel &mdash; live auf deinem Handy.
      <strong>Keine App, keine Anmeldung:</strong> Kamera auf den Code halten, antippen, fertig.
    </p>

    <div class="karten">
{links}
{rechts}
    </div>

    <div class="anleitung">
      <span class="schritt"><span class="nr">1</span>Kamera öffnen</span>
      <span class="pfeil">&rarr;</span>
      <span class="schritt"><span class="nr">2</span>Code anvisieren</span>
      <span class="pfeil">&rarr;</span>
      <span class="schritt"><span class="nr">3</span>Link antippen</span>
      <span class="pfeil">&rarr;</span>
      <span class="schritt"><span class="nr">4</span>Lesezeichen setzen</span>
    </div>

    <div class="schluss">
      <p class="ruf">Jetzt scannen &mdash; <em>dein Spiel läuft vielleicht schon.</em></p>
      <p class="klein">Kostenlos &middot; ohne Anmeldung<br>Die Turnierleitung hilft gern weiter.</p>
    </div>
  </div>
</body>
</html>
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kuerzel_kommt_aus_beiden_schreibweisen() {
        assert_eq!(
            kuerzel_aus_live_url("https://badhub.de/live?t=bvbb").as_deref(),
            Some("bvbb")
        );
        assert_eq!(
            kuerzel_aus_live_url("  https://badhub.de/live?t=bvbb-2026  ").as_deref(),
            Some("bvbb-2026")
        );
        assert_eq!(
            kuerzel_aus_live_url("https://badhub.de/live/bvbb").as_deref(),
            Some("bvbb")
        );
        assert_eq!(
            kuerzel_aus_live_url("https://badhub.de/live/bvbb/teilnehmer").as_deref(),
            Some("bvbb")
        );
        // Weitere Parameter stören nicht.
        assert_eq!(
            kuerzel_aus_live_url("https://badhub.de/live?lang=de&t=cup").as_deref(),
            Some("cup")
        );
    }

    #[test]
    fn unbrauchbare_angaben_ergeben_kein_kuerzel() {
        assert_eq!(kuerzel_aus_live_url(""), None);
        assert_eq!(kuerzel_aus_live_url("   "), None);
        assert_eq!(kuerzel_aus_live_url("https://badhub.de/"), None);
        // Kein Kürzel hinter /live/.
        assert_eq!(kuerzel_aus_live_url("https://badhub.de/live"), None);
        // Fremde Zeichen: eher Tippfehler oder Schmuggelversuch.
        assert_eq!(kuerzel_aus_live_url("https://badhub.de/live?t=a b"), None);
        assert_eq!(
            kuerzel_aus_live_url("https://badhub.de/live?t=<script>"),
            None
        );
        assert_eq!(
            kuerzel_aus_live_url("https://badhub.de/live?t=a/../../b"),
            None
        );
        assert_eq!(
            kuerzel_aus_live_url(&format!("https://badhub.de/live?t={}", "x".repeat(41))),
            None
        );
    }

    #[test]
    fn link_mit_guid_haengt_g_korrekt_an() {
        let g = Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        assert_eq!(
            link_mit_guid("https://badhub.de/live?t=bvbb", g),
            "https://badhub.de/live?t=bvbb&g=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        assert_eq!(
            link_mit_guid("https://badhub.de/live", g),
            "https://badhub.de/live?g=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        assert_eq!(
            link_mit_guid("https://badhub.de/live?t=bvbb", None),
            "https://badhub.de/live?t=bvbb"
        );
        // Leere Adresse bleibt leer — der Aufrufer meldet „keine Live-Seite".
        assert_eq!(link_mit_guid("", g), "");
    }

    #[test]
    fn aushang_ticker_zeigt_direkt_aufs_turnier() {
        let g = Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        let d = daten_aus(
            "https://badhub.de/live?t=bvbb&display=monitor",
            "Test",
            None,
            g,
        )
        .unwrap();
        assert_eq!(
            d.url_ticker,
            "https://badhub.de/live?t=bvbb&g=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        // Die Teilnehmerliste hängt am Verbandsschlüssel — badhub löst dort
        // über das zuletzt bepushte Turnier auf; unverändert.
        assert_eq!(d.url_teilnehmer, "https://badhub.de/live/bvbb/teilnehmer");
        // Ohne GUID wie bisher.
        let ohne = daten_aus("https://badhub.de/live?t=bvbb", "Test", None, None).unwrap();
        assert_eq!(ohne.url_ticker, "https://badhub.de/live?t=bvbb");
    }

    #[test]
    fn beide_adressen_werden_neu_gebaut() {
        // Die eingetragene Live-Seite ist die Teilnehmerliste: Der
        // Liveticker-Code darf trotzdem nicht dorthin zeigen.
        let d = daten_aus("https://badhub.de/live/bvbb/teilnehmer", "Test", None, None).unwrap();
        assert_eq!(d.url_ticker, "https://badhub.de/live?t=bvbb");
        assert_eq!(d.url_teilnehmer, "https://badhub.de/live/bvbb/teilnehmer");

        // Angehängte Parameter (Monitor-Ansicht, Hallenfilter) gehören nicht
        // auf ein Blatt für die ganze Halle.
        let mit_extra = daten_aus(
            "https://badhub.de/live?t=bvbb&display=monitor&halle=Halle+1",
            "Test",
            None,
            None,
        )
        .unwrap();
        assert_eq!(mit_extra.url_ticker, "https://badhub.de/live?t=bvbb");
        assert!(!mit_extra.url_ticker.contains("display"));
    }

    #[test]
    fn teilnehmerliste_bleibt_auf_derselben_installation() {
        let d = daten_aus("https://badhub.de/live?t=bvbb", "Test", None, None).unwrap();
        assert_eq!(d.url_ticker, "https://badhub.de/live?t=bvbb");
        assert_eq!(d.url_teilnehmer, "https://badhub.de/live/bvbb/teilnehmer");

        let lokal = daten_aus("http://localhost:8080/live?t=cup", "Test", None, None).unwrap();
        assert_eq!(
            lokal.url_teilnehmer,
            "http://localhost:8080/live/cup/teilnehmer"
        );

        // Ohne Schema lässt sich keine Basis bilden → kein Blatt.
        assert!(daten_aus("badhub.de/live?t=bvbb", "Test", None, None).is_none());
        assert!(daten_aus("", "Test", None, None).is_none());
    }

    #[test]
    fn blatt_traegt_beide_adressen_und_zwei_qr_codes() {
        let d = daten_aus("https://badhub.de/live?t=bvbb", "BVBB Open", None, None).unwrap();
        let html = render_html(&d).unwrap();
        assert_eq!(html.matches("<svg").count(), 2, "je Karte ein QR-Code");
        assert!(html.contains("badhub.de/live/bvbb/teilnehmer"));
        assert!(html.contains("badhub.de/live?t=bvbb"));
        assert!(html.contains("BVBB Open"));
        // Skriptfrei (ADR 0039).
        assert!(!html.contains("<script"));
        // A4 hoch.
        assert!(html.contains("size: A4 portrait"));
    }

    #[test]
    fn turniername_und_logo_landen_escaped_im_kopf() {
        let d = daten_aus(
            "https://badhub.de/live?t=bvbb",
            "<script>alert(1)</script>",
            Some("data:image/png;base64,aGVsbG8=".to_string()),
            None,
        )
        .unwrap();
        let html = render_html(&d).unwrap();
        assert!(!html.contains("<script"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains(r#"src="data:image/png;base64,aGVsbG8=""#));
    }

    #[test]
    fn ohne_turniername_und_logo_bleibt_der_kopf_schlicht() {
        let d = daten_aus("https://badhub.de/live?t=bvbb", "   ", None, None).unwrap();
        let html = render_html(&d).unwrap();
        assert!(!html.contains("class=\"turnier\""));
        assert!(!html.contains("<img"));
        assert!(html.contains("class=\"punkt\""));
    }

    #[test]
    fn qr_svg_ist_einbettbar_und_skalierbar() {
        let svg = qr_svg("https://badhub.de/live?t=bvbb").unwrap();
        assert!(svg.starts_with("<svg"), "keine XML-Deklaration: {svg:.40}");
        // Ohne viewBox ließe sich der Code im Blatt nicht auf 66 mm skalieren.
        assert!(svg.contains("viewBox"), "viewBox fehlt");
    }

    #[test]
    fn anzeige_url_kuerzt_schema_weg() {
        assert_eq!(
            anzeige_url("https://www.badhub.de/live?t=bvbb"),
            "badhub.de/live?t=bvbb"
        );
        assert_eq!(
            anzeige_url("http://localhost:8080/live/cup/teilnehmer"),
            "localhost:8080/live/cup/teilnehmer"
        );
    }
}
