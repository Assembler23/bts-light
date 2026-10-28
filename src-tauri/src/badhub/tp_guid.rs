//! Turnier-GUID aus den Bytes der TP-Datei — ohne Jet-Parser.
//!
//! Die Tabelle `Settings` der Turnierdatei ist eine name/value-Tabelle. Jet 4
//! legt ihre Textspalten als „komprimierten Unicode“ ab: Marke `FF FE`, dann
//! ein Byte je Zeichen. Name und Wert liegen im Datensatz direkt
//! hintereinander, deshalb steht die GUID im Dateibild als
//! `FF FE LinkCode FF FE <36 Zeichen>`. Messung 28.10.2026 an 26 TP-Dateien:
//! genau ein Treffer je Datei (siehe `docs/btp_protocol.md`).
//!
//! Warum kein Parser: Die GUID ist das einzige Feld, das bts-light aus der
//! Datei braucht; alles andere liest der Extraktor auf badhub. Ein Jet-Crate
//! gäbe es ohnehin nur mit Lizenzfrage.

/// Befund der Suche. `Mehrdeutig` entsteht, wenn eine alte Fassung der
/// Settings-Zeile noch im freien Seitenplatz liegt (Jet löscht nicht) und
/// anderes trägt — eine andere GUID oder gar keine. Welche Zeile die aktuelle
/// ist, verrät die Datei nicht; dann rät die App nicht. In den 30 gemessenen
/// Dateien kam der Anker je genau einmal vor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuidBefund {
    /// Kanonische GUID (Großschreibung), wie [`crate::config::kanonische_guid`].
    Gefunden(String),
    /// `LinkCode` vorhanden, aber ohne Wert — Turnier nicht mit turnier.de verknüpft.
    Leer,
    /// Mehrere Anker mit unterschiedlichem Inhalt: die gefundenen GUIDs,
    /// sortiert (ein Eintrag, wenn daneben ein leerer Anker lag).
    Mehrdeutig(Vec<String>),
    /// Kein `LinkCode` in der Datei.
    KeinAnker,
}

/// Turnier-Kennung aus der Datei, falls sie eindeutig ist **und** von der
/// eingestellten abweicht. Leer, mehrdeutig oder ohne Anker → `None`: dann
/// entscheidet der Server (`guid_mismatch`).
pub fn fremde_guid(bytes: &[u8], eingestellt: &str) -> Option<String> {
    match guid_aus_bytes(bytes) {
        GuidBefund::Gefunden(datei) if datei != eingestellt => Some(datei),
        _ => None,
    }
}

/// `99F55FAB-E301-…` → „99F55FAB…“, wie die Server-Meldung beide Kennungen
/// kürzt (und `guidKurz` im Setup-Wizard).
pub fn kurz(guid: &str) -> String {
    let kopf: String = guid.chars().take(8).collect();
    format!("{kopf}…")
}

/// Komprimierter Jet-Unicode (so in allen gemessenen Dateien).
const ANKER_KOMPRIMIERT: &[u8] = b"\xff\xfeLinkCode\xff\xfe";
/// Unkomprimiertes UCS-2 — nur zur Sicherheit, falls eine Spalte ohne
/// Unicode-Kompression angelegt wurde.
const ANKER_UTF16: &[u8] = b"L\0i\0n\0k\0C\0o\0d\0e\0";
/// `8-4-4-4-12` Hex mit Bindestrichen.
const GUID_ZEICHEN: usize = 36;

/// Sucht die Turnier-GUID hinter dem `LinkCode`-Anker. Liest nie über das
/// Ende hinaus; GUID-förmige Strings ohne Anker (Spieler-GUIDs, Jet-Interna)
/// zählen nicht. Kosten: ein Durchlauf über die Bytes, Millisekunden.
pub fn guid_aus_bytes(bytes: &[u8]) -> GuidBefund {
    let mut anker = 0usize;
    let mut gefunden: Vec<String> = Vec::new();
    for pos in fundstellen(bytes, ANKER_KOMPRIMIERT) {
        anker += 1;
        let wert = bytes[pos + ANKER_KOMPRIMIERT.len()..]
            .get(..GUID_ZEICHEN)
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(crate::config::kanonische_guid);
        gefunden.extend(wert);
    }
    // Die UCS-2-Form nur suchen, wenn die komprimierte fehlt — spart den
    // zweiten Durchlauf über die Datei im Regelfall.
    if anker == 0 {
        for pos in fundstellen(bytes, ANKER_UTF16) {
            anker += 1;
            let wert = bytes[pos + ANKER_UTF16.len()..]
                .get(..GUID_ZEICHEN * 2)
                .map(|s| {
                    s.chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect::<Vec<u16>>()
                })
                .and_then(|einheiten| String::from_utf16(&einheiten).ok())
                .and_then(|text| crate::config::kanonische_guid(&text));
            gefunden.extend(wert);
        }
    }
    let leere_anker = anker - gefunden.len();
    gefunden.sort();
    gefunden.dedup();
    match (anker, gefunden.len(), leere_anker) {
        (0, _, _) => GuidBefund::KeinAnker,
        (_, 0, _) => GuidBefund::Leer,
        (_, 1, 0) => GuidBefund::Gefunden(gefunden.remove(0)),
        _ => GuidBefund::Mehrdeutig(gefunden),
    }
}

/// Alle Anfangspositionen von `muster` in `bytes`. Springt per `position` von
/// Erstbyte zu Erstbyte statt jedes Fenster zu vergleichen — bei 3 MB der
/// Unterschied zwischen spürbar und unmerklich.
fn fundstellen(bytes: &[u8], muster: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let Some(&erstes) = muster.first() else {
        return out;
    };
    let mut ab = 0;
    while ab + muster.len() <= bytes.len() {
        let Some(rel) = bytes[ab..].iter().position(|&b| b == erstes) else {
            break;
        };
        let pos = ab + rel;
        if bytes[pos..].starts_with(muster) {
            out.push(pos);
        }
        ab = pos + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUID: &str = "49d86707-dd27-4b1f-8690-13a109dce175";
    const GUID_GROSS: &str = "49D86707-DD27-4B1F-8690-13A109DCE175";
    const ANDERE: &str = "99F55FAB-E301-41A2-A165-1D162CB84FC1";

    /// Datensatz wie in der Datei: Marke + Name + Marke + Wert + Folgebytes.
    fn zeile(wert: &str) -> Vec<u8> {
        let mut v = b"\xff\xfeLinkCode\xff\xfe".to_vec();
        v.extend_from_slice(wert.as_bytes());
        v.extend_from_slice(b"6\x00\x10\x00\x06\x00");
        v
    }

    fn datei(teile: &[&[u8]]) -> Vec<u8> {
        let mut v = vec![0u8; 100];
        for t in teile {
            v.extend_from_slice(t);
            v.extend_from_slice(&[0u8; 50]);
        }
        v
    }

    #[test]
    fn gefunden_komprimierter_unicode_wird_kanonisch() {
        let b = datei(&[&zeile(GUID)]);
        assert_eq!(guid_aus_bytes(&b), GuidBefund::Gefunden(GUID_GROSS.into()));
    }

    #[test]
    fn gefunden_auch_in_utf16() {
        let mut z = Vec::new();
        for c in "LinkCode".encode_utf16() {
            z.extend_from_slice(&c.to_le_bytes());
        }
        for c in GUID_GROSS.encode_utf16() {
            z.extend_from_slice(&c.to_le_bytes());
        }
        let b = datei(&[&z]);
        assert_eq!(guid_aus_bytes(&b), GuidBefund::Gefunden(GUID_GROSS.into()));
    }

    #[test]
    fn ohne_anker_zaehlt_keine_fremde_guid() {
        // Spieler- und Jet-GUIDs stehen zuhauf in der Datei — ohne Anker nichts wert.
        let b = datei(&[b"\xff\xfePlayerGuid\xff\xfe", GUID_GROSS.as_bytes()]);
        assert_eq!(guid_aus_bytes(&b), GuidBefund::KeinAnker);
    }

    #[test]
    fn anker_ohne_wert_ist_leer() {
        let b = datei(&[b"\xff\xfeLinkCode\xff\xfeSiteCode\xff\xfe123"]);
        assert_eq!(guid_aus_bytes(&b), GuidBefund::Leer);
    }

    #[test]
    fn anker_am_dateiende_ohne_panik() {
        let mut b = datei(&[]);
        b.extend_from_slice(b"\xff\xfeLinkCode\xff\xfe49D8");
        assert_eq!(guid_aus_bytes(&b), GuidBefund::Leer);
    }

    #[test]
    fn zwei_anker_mit_derselben_guid_sind_eindeutig() {
        let b = datei(&[&zeile(GUID), &zeile(GUID_GROSS)]);
        assert_eq!(guid_aus_bytes(&b), GuidBefund::Gefunden(GUID_GROSS.into()));
    }

    #[test]
    fn leerer_anker_neben_gefuellter_zeile_ist_mehrdeutig() {
        // Welche Zeile die aktuelle ist, sagt die Datei nicht: Eine entlinkte
        // Datei mit alter GUID im freien Seitenplatz sähe genauso aus wie
        // eine frisch verknüpfte mit leerer Altfassung. Also nicht raten —
        // in beiden Reihenfolgen.
        let b = datei(&[b"\xff\xfeLinkCode\xff\xfeSiteCode", &zeile(GUID)]);
        assert_eq!(
            guid_aus_bytes(&b),
            GuidBefund::Mehrdeutig(vec![GUID_GROSS.into()])
        );
        let b = datei(&[&zeile(GUID), b"\xff\xfeLinkCode\xff\xfeSiteCode"]);
        assert_eq!(
            guid_aus_bytes(&b),
            GuidBefund::Mehrdeutig(vec![GUID_GROSS.into()])
        );
    }

    #[test]
    fn ucs2_form_zaehlt_nur_ohne_komprimierten_anker() {
        // Gibt es den komprimierten Anker, ist die zweite Suche überflüssig
        // (und ein UCS-2-Treffer mit anderer GUID wäre kein echter Datensatz).
        let mut z = Vec::new();
        for c in "LinkCode".encode_utf16().chain(ANDERE.encode_utf16()) {
            z.extend_from_slice(&c.to_le_bytes());
        }
        let b = datei(&[&zeile(GUID), &z]);
        assert_eq!(guid_aus_bytes(&b), GuidBefund::Gefunden(GUID_GROSS.into()));
    }

    #[test]
    fn fremde_guid_nur_bei_eindeutig_abweichender_kennung() {
        assert_eq!(
            fremde_guid(&datei(&[&zeile(GUID)]), ANDERE),
            Some(GUID_GROSS.to_string())
        );
        assert_eq!(fremde_guid(&datei(&[&zeile(GUID)]), GUID_GROSS), None);
        // Unklar oder ohne Anker → der Server entscheidet.
        assert_eq!(fremde_guid(&datei(&[]), ANDERE), None);
        assert_eq!(
            fremde_guid(&datei(&[&zeile(GUID), &zeile(ANDERE)]), ANDERE),
            None
        );
    }

    #[test]
    fn kurz_nimmt_acht_zeichen_und_ellipse() {
        assert_eq!(kurz(GUID_GROSS), "49D86707…");
        assert_eq!(kurz("ABC"), "ABC…");
    }

    #[test]
    fn zwei_verschiedene_guids_sind_mehrdeutig_und_sortiert() {
        let b = datei(&[&zeile(ANDERE), &zeile(GUID)]);
        assert_eq!(
            guid_aus_bytes(&b),
            GuidBefund::Mehrdeutig(vec![GUID_GROSS.into(), ANDERE.into()])
        );
    }

    /// Sonde gegen eine echte Turnierdatei: `BTS_TP_DATEI=<Pfad> cargo test
    /// guid_aus_echter_datei -- --ignored --nocapture`. Druckt den Befund.
    #[test]
    #[ignore]
    fn guid_aus_echter_datei() {
        let pfad = std::env::var("BTS_TP_DATEI").expect("BTS_TP_DATEI setzen");
        let bytes = std::fs::read(&pfad).unwrap();
        let start = std::time::Instant::now();
        let befund = guid_aus_bytes(&bytes);
        println!(
            "{} Bytes, {:?} in {} ms",
            bytes.len(),
            befund,
            start.elapsed().as_millis()
        );
        assert!(matches!(befund, GuidBefund::Gefunden(_)), "{befund:?}");
    }

    #[test]
    fn leere_datei_und_nur_kopf() {
        assert_eq!(guid_aus_bytes(&[]), GuidBefund::KeinAnker);
        assert_eq!(
            guid_aus_bytes(crate::badhub::tp_upload::JET_KOPF),
            GuidBefund::KeinAnker
        );
    }
}
