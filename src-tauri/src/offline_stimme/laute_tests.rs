use super::*;

fn lex() -> Lexikon {
    Lexikon::aus_tsv(
        "Feld\t/fɛlt/\n\
         drei\t/dʁaɪ̯/\n\
         gegen\t/ˈɡeːɡn̩/, /ˈɡeːɡŋ̍/\n\
         Herren\t/ˈhɛʁən/\n\
         Doppel\t/ˈdɔpl̩/\n\
         Spiel\t/ʃpiːl/\n\
         Satz\t/zat͡s/\n\
         Freude\t/ˈfʁɔʏ̯də/\n\
         auf\t/aʊ̯f/\n\
         Aufschlag\t/ˈaʊ̯fˌʃlaːk/\n\
         ein\t/aɪ̯n/\n\
         und\t/ʊnt/\n\
         zwanzig\t/ˈt͡svant͡sɪç/\n\
         gut gemacht\t/ɡuːt ɡəˈmaxt/\n",
    )
}

fn lautierer<'a>(l: &'a Lexikon, a: &'a HashMap<String, String>) -> Lautierer<'a> {
    Lautierer {
        lexikon: l,
        ausnahmen: a,
    }
}

#[test]
fn wiktionary_wird_zur_espeak_schreibweise() {
    assert_eq!(wiktionary_zu_espeak("/fɛlt/"), "fɛlt");
    assert_eq!(wiktionary_zu_espeak("/dʁaɪ̯/"), "dʁaɪ");
    // Erste Variante, silbisches n mit Schwa, Betonung vor dem Vokal.
    assert_eq!(wiktionary_zu_espeak("/ˈɡeːɡn̩/, /ˈɡeːɡŋ̍/"), "ɡˈeːɡən");
    assert_eq!(wiktionary_zu_espeak("/ˈaʊ̯fˌʃlaːk/"), "ˈaʊfʃlˌaːk");
    assert_eq!(wiktionary_zu_espeak("/zat͡s/"), "zats");
    assert_eq!(wiktionary_zu_espeak("/ˈfʁɔʏ̯də/"), "fʁˈɔødə");
    // Klammern weg, Inhalt bleibt; Knacklaut weg.
    assert_eq!(wiktionary_zu_espeak("/ʔɪm(ə)/"), "ɪmə");
}

#[test]
fn mehrwort_eintraege_werden_uebersprungen() {
    let l = lex();
    assert!(l.finde("gut gemacht").is_none());
    assert_eq!(l.finde("feld"), Some("fɛlt"));
    assert_eq!(l.finde("FELD"), Some("fɛlt"));
}

#[test]
fn reihenfolge_ausnahme_lexikon_zusammengesetzt_geschaetzt() {
    let l = lex();
    let mut a = HashMap::new();
    a.insert("feld".to_string(), "fˈeːlt".to_string());
    let t = lautierer(&l, &a);
    let w = t.wort("Feld");
    assert_eq!((w.laute.as_str(), w.quelle), ("fˈeːlt", Quelle::Ausnahme));
    let w = t.wort("Spiel");
    assert_eq!(w.quelle, Quelle::Lexikon);
    let w = t.wort("Herrendoppel");
    assert_eq!(w.quelle, Quelle::Zusammengesetzt);
    // Zweiter Teil nur noch nebenbetont.
    assert_eq!(w.laute, "hˈɛʁəndˌɔpəl");
    let w = t.wort("Kowalczyk");
    assert_eq!(w.quelle, Quelle::Geschaetzt);
}

#[test]
fn fugen_s_wird_erkannt() {
    let l = lex();
    let a = HashMap::new();
    let w = lautierer(&l, &a).wort("Spielsatz");
    assert_eq!(w.quelle, Quelle::Zusammengesetzt);
    assert_eq!(w.laute, "ʃpiːlzats");
    let mut l2 = lex();
    l2.einfuegen("Satzspiel", "x");
    let w = lautierer(&l2, &a).wort("Feldsspiel");
    assert_eq!(w.laute, "fɛltsʃpiːl");
}

/// K18: Jedes Wort mit Buchstaben wird gesprochen, auch seltene Namen.
#[test]
fn kein_wort_wird_verschluckt() {
    let leer = Lexikon::default();
    let a = HashMap::new();
    let t = lautierer(&leer, &a);
    for name in [
        "Kowalczyk",
        "Brzęczyszczykiewicz",
        "Nguyen",
        "Thị",
        "Øster",
        "Xu",
        "Ng",
        "Schmidt",
        "Wojciechowski",
        "Yannick",
        "Ćwik",
        "Hrbek",
    ] {
        let w = t.wort(&ohne_akzente(name));
        assert!(
            w.laute.chars().any(ist_vokal),
            "{name} ohne Vokal: {:?}",
            w.laute
        );
        assert!(w.laute.contains('ˈ'), "{name} ohne Betonung: {:?}", w.laute);
    }
}

#[test]
fn schaetzer_kennt_die_haeufigen_regeln() {
    assert_eq!(schaetzen("schmidt"), "ʃmˈɪt");
    assert_eq!(schaetzen("müller"), "mˈʏlɐ");
    assert_eq!(schaetzen("weber"), "vˈeːbɐ");
    assert_eq!(schaetzen("bach"), "bˈax");
    assert_eq!(schaetzen("fischer"), "fˈɪʃɐ");
    assert_eq!(schaetzen("stein"), "ʃtˈaɪn");
    assert_eq!(schaetzen("berger"), "bˈɛʁɡɐ");
    assert_eq!(schaetzen("lehmann"), "lˈeːman");
    assert!(schaetzen("koenig").chars().any(ist_vokal));
    // Namen auf der ersten Silbe betont, auch mit „Vorsilbe".
    assert!(schaetzen("gerhardt").starts_with("ɡˈɛ"));
    assert!(schaetzen("bertram").starts_with("bˈɛ"));
    assert_eq!(schaetzen("harrison"), "hˈaʁɪzɔn");
    assert_eq!(schaetzen("minh"), "mˈɪn");
    assert_eq!(schaetzen("thompson"), "tˈɔmpsɔn");
    assert_eq!(schaetzen("kowalczyk"), "kˈoːvaltʃʏk");
    assert_eq!(schaetzen("szymanska"), "ʃˈyːmanska");
    assert!(schaetzen("brzezinski").starts_with("bʒ"));
    assert_eq!(schaetzen("schwarz"), "ʃvˈaʁts");
    assert_eq!(schaetzen("hughes"), "hˈʊɡəs");
    assert_eq!(schaetzen("wright"), "vʁˈɪt");
    // Nur Konsonanten: buchstabiert.
    assert_eq!(schaetzen("ng"), "ɛnɡˈeː");
}

#[test]
fn zahlen_werden_zu_woertern() {
    assert_eq!(zahlwort(0), "null");
    assert_eq!(zahlwort(1), "eins");
    assert_eq!(zahlwort(12), "zwölf");
    assert_eq!(zahlwort(21), "ein und zwanzig");
    assert_eq!(zahlwort(30), "dreißig");
    assert_eq!(zahlwort(101), "ein hundert eins");
    assert_eq!(zahlwort(1000), "ein tausend");
    assert_eq!(zahlwort(2026), "zwei tausend sechs und zwanzig");
    assert_eq!(zahlwort(21_000), "ein und zwanzig tausend");
    assert_eq!(zahlwort(1_234_567), "eins zwei drei vier fünf sechs sieben");
}

#[test]
fn saetze_mit_satzzeichen_und_zahlen() {
    let l = lex();
    let a = HashMap::new();
    let s = lautierer(&l, &a).saetze("Feld 3: Herren gegen drei. Aufschlag 21");
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].laute, "fɛlt dʁaɪ, hˈɛʁən ɡˈeːɡən dʁaɪ.");
    assert_eq!(s[0].quellen.len(), 5);
    assert!(s[1].laute.starts_with("ˈaʊfʃlˌaːk aɪn ʊnt tsvˈantsɪç"));
    assert!(s[1].laute.ends_with('.'));
    // Bindestrich trennt, leere Sätze fallen weg.
    let s = lautierer(&l, &a).saetze("... Feld-Spiel!");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].laute, "fɛlt ʃpiːl!");
}

#[test]
fn akzente_auf_grundbuchstaben() {
    assert_eq!(ohne_akzente("Brzęczyszczykiewicz"), "Brzeczyszczykiewicz");
    assert_eq!(ohne_akzente("Łukasz"), "Lukasz");
    assert_eq!(ohne_akzente("Nguyễn Thị"), "Nguyen Thi");
    assert_eq!(ohne_akzente("Müller Straße"), "Müller Straße");
}

/// Unsinnige Eingaben: kein Absturz; wo Buchstaben oder Ziffern stehen,
/// kommt ein Satz mit Lauten heraus (K18).
#[test]
fn unsinnige_eingaben_stuerzen_nicht_ab() {
    let leer = Lexikon::default();
    let a = HashMap::new();
    let t = lautierer(&leer, &a);
    for (text, satz_erwartet) in [
        ("", false),
        ("...!?,;:", false),
        ("   ", false),
        ("—–'\"«»", false),
        ("12345678901234567890123", true),
        ("Nguye\u{0302}\u{0303}n", true),
        ("\u{0301}\u{0301}", false),
        ("北京", true),
        ("Ж", true),
        ("x", true),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            true,
        ),
        ("Feld\u{0000}3", true),
    ] {
        let s = t.saetze(text);
        assert_eq!(!s.is_empty(), satz_erwartet, "{text:?}");
        for satz in &s {
            assert!(
                satz.laute.chars().any(ist_vokal),
                "{text:?}: {:?}",
                satz.laute
            );
        }
    }
    // NFD bleibt ein Wort.
    assert_eq!(t.saetze("Nguye\u{0302}\u{0303}n")[0].quellen.len(), 1);
    // Überlange Zahl Ziffer für Ziffer.
    assert_eq!(ziffernweise("007"), "null null sieben");
}
