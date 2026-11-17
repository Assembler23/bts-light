//! Text → Laute für die Offline-Stimme (Weg V1, ADR 0104).
//!
//! Die Piper-Stimme `thorsten` ist mit Lautschrift im Stil von espeak-ng
//! trainiert. espeak-ng ist GPL und kommt nicht in die Programmdatei; die
//! Lautschrift entsteht deshalb hier:
//!
//! 1. **Ausnahmeliste** (Name → Laute) — hat Vorrang.
//! 2. **Lexikon** (freies Aussprache-Lexikon aus dem Wiktionary, CC-BY-SA),
//!    beim Laden in die espeak-Schreibweise gebracht.
//! 3. **Zusammengesetztes Wort** aus Lexikon-Teilen („Herrendoppel").
//! 4. **Schätzer** nach deutschen Ausspracheregeln — für Namen, die nirgends
//!    stehen. Er liefert immer etwas: Ein Wort wird nie verschluckt (K18).
//!
//! Zahlen werden vorher zu Wörtern. Ob dieser Weg gut genug klingt,
//! entscheidet der Hörvergleich der Messung E0 (`tests/offline_stimme_probe.rs`).

use std::collections::HashMap;

/// Woher die Laute eines Wortes kamen — für die Auswertung der Messung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quelle {
    Ausnahme,
    Lexikon,
    Zusammengesetzt,
    Geschaetzt,
}

impl Quelle {
    /// Festes Kennwort fürs Messprotokoll.
    pub fn wort(self) -> &'static str {
        match self {
            Self::Ausnahme => "ausnahme",
            Self::Lexikon => "lexikon",
            Self::Zusammengesetzt => "zusammengesetzt",
            Self::Geschaetzt => "geschaetzt",
        }
    }
}

/// Vokalzeichen der Lautschrift (für Betonung und Silben).
const VOKALE: &str = "aeiouyøœɛɪɔʊʏəɐæɑɒʌɜɞɘɵɤɯɨʉɚ";

fn ist_vokal(c: char) -> bool {
    VOKALE.contains(c)
}

/// Aussprache-Lexikon: Schreibweise → Laute in espeak-Schreibweise.
#[derive(Default)]
pub struct Lexikon {
    eintraege: HashMap<String, String>,
}

impl Lexikon {
    /// Liest das Wiktionary-Lexikon im Format von `ipa-dict`
    /// (`Wort<TAB>/laute/, /laute2/` je Zeile). Mehrwort-Einträge und
    /// unlesbare Zeilen werden übersprungen.
    pub fn aus_tsv(text: &str) -> Self {
        let mut eintraege = HashMap::new();
        for zeile in text.lines() {
            let Some((wort, ipa)) = zeile.split_once('\t') else {
                continue;
            };
            let wort = wort.trim();
            if wort.is_empty() || wort.contains(' ') {
                continue;
            }
            let laute = wiktionary_zu_espeak(ipa);
            if laute.is_empty() {
                continue;
            }
            eintraege.entry(wort.to_string()).or_insert(laute);
        }
        Self { eintraege }
    }

    pub fn einfuegen(&mut self, wort: &str, laute: &str) {
        self.eintraege.insert(wort.to_string(), laute.to_string());
    }

    pub fn len(&self) -> usize {
        self.eintraege.len()
    }

    pub fn is_empty(&self) -> bool {
        self.eintraege.is_empty()
    }

    /// Sucht genau, dann klein, dann mit großem Anfangsbuchstaben.
    fn finde(&self, wort: &str) -> Option<&str> {
        if let Some(l) = self.eintraege.get(wort) {
            return Some(l);
        }
        let klein = wort.to_lowercase();
        if let Some(l) = self.eintraege.get(&klein) {
            return Some(l);
        }
        let mut z = klein.chars();
        let erster = z.next()?;
        let gross: String = erster.to_uppercase().chain(z).collect();
        self.eintraege.get(&gross).map(String::as_str)
    }
}

/// Bringt eine Wiktionary-Lautschrift in die Schreibweise von espeak-ng:
/// erste Variante, ohne Klammern und Bindebögen, silbische Konsonanten mit
/// Schwa, Betonungszeichen vor dem Vokal statt am Silbenanfang.
pub fn wiktionary_zu_espeak(ipa: &str) -> String {
    let erste = ipa.split(',').next().unwrap_or("").trim();
    let mut roh = String::new();
    for c in erste.chars() {
        match c {
            '/' | '[' | ']' | '(' | ')' | '͡' | '̯' | '‿' | '|' | '‖' | '.' | 'ʔ' | '̥' | '̺' | '̝'
            | '̊' | '̑' | '̆' | '̞' | '̚' | '͜' => {}
            'g' => roh.push('ɡ'),
            'r' => roh.push('ʁ'),
            // Silbischer Konsonant: Schwa davor („gegen" → ɡeːɡən).
            '̩' | '̍' => {
                if let Some(k) = roh.pop() {
                    roh.push('ə');
                    roh.push(k);
                }
            }
            c => roh.push(c),
        }
    }
    // espeak schreibt „eu" als ɔø.
    let roh = roh.replace("ɔʏ", "ɔø").replace("ɔɪ", "ɔø");
    betonung_vor_vokal(&roh)
}

/// Schiebt jedes Betonungszeichen vor den nächsten Vokal.
fn betonung_vor_vokal(s: &str) -> String {
    let mut aus = String::with_capacity(s.len() + 2);
    let mut offen: Option<char> = None;
    for c in s.chars() {
        match c {
            'ˈ' | 'ˌ' => {
                // Zwei Zeichen ohne Vokal dazwischen: das stärkere gilt.
                offen = match (offen, c) {
                    (Some('ˈ'), _) => Some('ˈ'),
                    _ => Some(c),
                };
            }
            c if ist_vokal(c) => {
                if let Some(b) = offen.take() {
                    aus.push(b);
                }
                aus.push(c);
            }
            c => aus.push(c),
        }
    }
    aus
}

/// Laute eines Wortes samt Herkunft.
pub struct Wortlaut {
    pub laute: String,
    pub quelle: Quelle,
}

/// Wandelt Text in Laute. Ausnahmen sind klein geschrieben hinterlegt.
pub struct Lautierer<'a> {
    pub lexikon: &'a Lexikon,
    pub ausnahmen: &'a HashMap<String, String>,
}

/// Ein Satz in Lautschrift, fertig für die Engine.
pub struct Satz {
    pub laute: String,
    pub quellen: Vec<Quelle>,
}

impl Lautierer<'_> {
    /// Ein einzelnes Wort (nur Buchstaben).
    pub fn wort(&self, wort: &str) -> Wortlaut {
        let klein = wort.to_lowercase();
        if let Some(l) = self.ausnahmen.get(&klein) {
            return Wortlaut {
                laute: l.clone(),
                quelle: Quelle::Ausnahme,
            };
        }
        if let Some(l) = self.lexikon.finde(wort) {
            return Wortlaut {
                laute: l.to_string(),
                quelle: Quelle::Lexikon,
            };
        }
        if let Some(l) = self.zusammengesetzt(&klein, 0) {
            return Wortlaut {
                laute: l,
                quelle: Quelle::Zusammengesetzt,
            };
        }
        Wortlaut {
            laute: schaetzen(&klein),
            quelle: Quelle::Geschaetzt,
        }
    }

    /// Zerlegt ein Wort in Lexikon-Teile (ab drei Buchstaben, Fugen-s
    /// erlaubt); längster Anfang zuerst. Ab dem zweiten Teil wird die
    /// Hauptbetonung zur Nebenbetonung.
    fn zusammengesetzt(&self, wort: &str, tiefe: u8) -> Option<String> {
        let zeichen: Vec<char> = wort.chars().collect();
        // Über 40 Buchstaben (Fehleingabe) nicht zerlegen — die Suche
        // wüchse sonst stark; der Schätzer liest es trotzdem.
        if tiefe > 3 || zeichen.len() < 6 || zeichen.len() > 40 {
            return None;
        }
        for schnitt in (3..=zeichen.len() - 3).rev() {
            let vorn: String = zeichen[..schnitt].iter().collect();
            let Some(l_vorn) = self.lexikon.finde(&vorn) else {
                continue;
            };
            let mut rest: String = zeichen[schnitt..].iter().collect();
            let mut fuge = "";
            let l_rest = match self.teil(&rest, tiefe) {
                Some(l) => Some(l),
                None if rest.starts_with('s') && rest.chars().count() > 3 => {
                    rest = rest[1..].to_string();
                    fuge = "s";
                    self.teil(&rest, tiefe)
                }
                None => None,
            };
            if let Some(l_rest) = l_rest {
                return Some(format!("{l_vorn}{fuge}{}", l_rest.replace('ˈ', "ˌ")));
            }
        }
        None
    }

    fn teil(&self, rest: &str, tiefe: u8) -> Option<String> {
        if rest.chars().count() < 3 {
            return None;
        }
        self.lexikon
            .finde(rest)
            .map(str::to_string)
            .or_else(|| self.zusammengesetzt(rest, tiefe + 1))
    }

    /// Ganzer Text → Sätze in Lautschrift. Satzzeichen bleiben als Pausen
    /// stehen; Zahlen werden zu Wörtern, Bindestriche trennen Wörter.
    pub fn saetze(&self, text: &str) -> Vec<Satz> {
        let mut saetze = Vec::new();
        let mut laute = String::new();
        let mut quellen = Vec::new();
        let mut wort = String::new();
        let mut zahl = String::new();
        let abschliessen = |wort: &mut String,
                            zahl: &mut String,
                            laute: &mut String,
                            quellen: &mut Vec<Quelle>| {
            if !zahl.is_empty() {
                // Zu lang für u64: Ziffer für Ziffer (nie verschlucken, K18).
                let w = zahl
                    .parse::<u64>()
                    .map(zahlwort)
                    .unwrap_or_else(|_| ziffernweise(zahl));
                zahl.clear();
                for teil in w.split(' ').filter(|t| !t.is_empty()) {
                    let wl = self.wort(teil);
                    trennen(laute);
                    laute.push_str(&wl.laute);
                    quellen.push(wl.quelle);
                }
            }
            if !wort.is_empty() {
                let wl = self.wort(&ohne_akzente(wort));
                wort.clear();
                trennen(laute);
                laute.push_str(&wl.laute);
                quellen.push(wl.quelle);
            }
        };
        for c in text.chars() {
            // Zerlegte Schreibweise (NFD): Akzentzeichen gehören zum Wort
            // und fallen weg, statt es zu zerreißen („Nguyễn").
            if ('\u{300}'..='\u{36f}').contains(&c) {
                continue;
            }
            if c.is_alphabetic() {
                if !zahl.is_empty() {
                    abschliessen(&mut wort, &mut zahl, &mut laute, &mut quellen);
                }
                wort.push(c);
            } else if c.is_ascii_digit() {
                if !wort.is_empty() {
                    abschliessen(&mut wort, &mut zahl, &mut laute, &mut quellen);
                }
                zahl.push(c);
            } else {
                abschliessen(&mut wort, &mut zahl, &mut laute, &mut quellen);
                match c {
                    ',' | ';' | ':' => laute.push(','),
                    '.' | '!' | '?' => {
                        laute.push(c);
                        if laute.chars().any(ist_vokal) {
                            saetze.push(Satz {
                                laute: std::mem::take(&mut laute),
                                quellen: std::mem::take(&mut quellen),
                            });
                        } else {
                            laute.clear();
                            quellen.clear();
                        }
                    }
                    _ => {}
                }
            }
        }
        abschliessen(&mut wort, &mut zahl, &mut laute, &mut quellen);
        if laute.chars().any(ist_vokal) {
            if !laute.ends_with(['.', '!', '?']) {
                laute.push('.');
            }
            saetze.push(Satz { laute, quellen });
        }
        saetze
    }
}

/// Leerzeichen vor dem nächsten Wort, außer am Satzanfang.
fn trennen(laute: &mut String) {
    if !laute.is_empty() && !laute.ends_with(' ') {
        laute.push(' ');
    }
}

/// Fremde Buchstaben mit Zeichen auf ihren deutschen Grundbuchstaben
/// (Umlaute und ß bleiben). Reicht für polnische, tschechische,
/// vietnamesische und französische Namen, damit der Schätzer sie liest.
pub fn ohne_akzente(wort: &str) -> String {
    wort.chars()
        .map(|c| {
            let klein = c.to_lowercase().next().unwrap_or(c);
            let basis = match klein {
                'à' | 'á' | 'â' | 'ã' | 'å' | 'ą' | 'ă' | 'ā' | 'ạ' | 'ả' | 'ấ' | 'ầ' | 'ẩ'
                | 'ẫ' | 'ậ' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => 'a',
                'ç' | 'ć' | 'č' => 'c',
                'ď' | 'đ' => 'd',
                'è' | 'é' | 'ê' | 'ë' | 'ę' | 'ě' | 'ē' | 'ẹ' | 'ẻ' | 'ẽ' | 'ế' | 'ề' | 'ể'
                | 'ễ' | 'ệ' => 'e',
                'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ị' | 'ỉ' | 'ĩ' => 'i',
                'ł' => 'l',
                'ñ' | 'ń' | 'ň' => 'n',
                'ò' | 'ó' | 'ô' | 'õ' | 'ō' | 'ơ' | 'ọ' | 'ỏ' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ'
                | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => 'o',
                'ø' => 'ö',
                'ř' => 'r',
                'ś' | 'š' | 'ş' => 's',
                'ť' | 'ţ' => 't',
                'ù' | 'ú' | 'û' | 'ů' | 'ū' | 'ư' | 'ụ' | 'ủ' | 'ũ' | 'ứ' | 'ừ' | 'ử' | 'ữ'
                | 'ự' => 'u',
                'ý' | 'ỳ' | 'ỵ' | 'ỷ' | 'ỹ' => 'y',
                'ź' | 'ż' | 'ž' => 'z',
                andere => andere,
            };
            if c.is_uppercase() && basis != klein {
                basis.to_uppercase().next().unwrap_or(basis)
            } else if c.is_uppercase() {
                c
            } else {
                basis
            }
        })
        .collect()
}

// --- Schätzer ---------------------------------------------------------------

#[derive(Debug, Clone)]
enum Seg {
    /// Vokal: kurze und lange Form, ob „e" (wird unbetont zu ə), ob die
    /// Länge feststeht.
    Vokal {
        kurz: &'static str,
        lang: &'static str,
        ist_e: bool,
        fest: Option<bool>,
    },
    Kons(&'static str),
    /// „r" — nach dem Vokal vor Konsonant/Wortende wird es zu ɐ.
    R,
}

/// Schätzt die Laute eines (klein geschriebenen) Wortes nach deutschen
/// Regeln. Liefert für jedes Wort mit Buchstaben eine nicht leere Folge.
pub fn schaetzen(wort: &str) -> String {
    let z: Vec<char> = wort
        .chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_alphabetic())
        .collect();
    if z.is_empty() {
        return String::new();
    }
    let segs = zerlegen(&z);
    let aus = aussprechen(&z, &segs);
    if aus.chars().any(ist_vokal) {
        aus
    } else {
        // Nur Konsonanten (Abkürzung): Buchstaben einzeln wie „ka, el".
        buchstabieren(&z)
    }
}

fn buchstabieren(z: &[char]) -> String {
    let teile: Vec<&str> = z
        .iter()
        .map(|c| match c {
            'b' => "beː",
            'c' => "tseː",
            'd' => "deː",
            'f' => "ɛf",
            'g' => "ɡeː",
            'h' => "haː",
            'j' => "jɔt",
            'k' => "kaː",
            'l' => "ɛl",
            'm' => "ɛm",
            'n' => "ɛn",
            'p' => "peː",
            'q' => "kuː",
            'r' => "ɛʁ",
            's' => "ɛs",
            't' => "teː",
            'v' => "faʊ",
            'w' => "veː",
            'x' => "ɪks",
            'z' => "tsɛt",
            _ => "ə",
        })
        .collect();
    let mut aus = String::new();
    let n = teile.len();
    for (i, t) in teile.iter().enumerate() {
        // Betonung auf dem letzten Buchstaben, wie beim Buchstabieren üblich.
        if i + 1 == n {
            aus.push_str(&betonung_vor_vokal(&format!("ˈ{t}")));
        } else {
            aus.push_str(t);
        }
    }
    aus
}

fn zerlegen(z: &[char]) -> Vec<(Seg, usize)> {
    let n = z.len();
    let ab = |i: usize, s: &str| -> bool {
        let m: Vec<char> = s.chars().collect();
        i + m.len() <= n && z[i..i + m.len()] == m[..]
    };
    let vokal_buchstabe = |c: char| "aeiouäöüy".contains(c);
    let mut segs: Vec<(Seg, usize)> = Vec::new();
    let mut i = 0;
    // Vorheriger Vokal hinten (für „ch" nach a/o/u/au).
    let mut hinten = false;
    while i < n {
        let c = z[i];
        let v = |kurz, lang, ist_e, fest| Seg::Vokal {
            kurz,
            lang,
            ist_e,
            fest,
        };
        // „rz" nur am Anfang oder nach Konsonant polnisch („Brzezinski"),
        // nach Vokal bleibt es deutsch („Schwarz").
        let rz_polnisch = ab(i, "rz") && (i == 0 || !vokal_buchstabe(z[i - 1]));
        let (seg, len): (Seg, usize) = if ab(i, "szcz") {
            (Seg::Kons("ʃtʃ"), 4)
        } else if ab(i, "tsch") {
            (Seg::Kons("tʃ"), 4)
        } else if ab(i, "sch") {
            (Seg::Kons("ʃ"), 3)
        } else if ab(i, "cz") {
            (Seg::Kons("tʃ"), 2)
        } else if ab(i, "sz") {
            (Seg::Kons("ʃ"), 2)
        } else if rz_polnisch {
            (Seg::Kons("ʒ"), 2)
        } else if ab(i, "gh") {
            // Englisch: „Hughes" ɡ, „Wright"/„Leigh" stumm.
            let stumm = i + 2 >= n || z[i + 2] == 't';
            (Seg::Kons(if stumm { "" } else { "ɡ" }), 2)
        } else if ab(i, "chs") {
            (Seg::Kons("ks"), 3)
        } else if ab(i, "ch") {
            (Seg::Kons(if hinten { "x" } else { "ç" }), 2)
        } else if ab(i, "ck") {
            (Seg::Kons("k"), 2)
        } else if ab(i, "ph") {
            (Seg::Kons("f"), 2)
        } else if ab(i, "th") || ab(i, "dt") {
            (Seg::Kons("t"), 2)
        } else if ab(i, "tz") {
            (Seg::Kons("ts"), 2)
        } else if ab(i, "qu") {
            (Seg::Kons("kv"), 2)
        } else if ab(i, "ng") {
            (Seg::Kons("ŋ"), 2)
        } else if ab(i, "nk") {
            (Seg::Kons("ŋk"), 2)
        } else if ab(i, "pf") {
            (Seg::Kons("pf"), 2)
        } else if ab(i, "ie") {
            (v("iː", "iː", false, Some(true)), 2)
        } else if ab(i, "ei") || ab(i, "ai") || ab(i, "ey") || ab(i, "ay") {
            (v("aɪ", "aɪ", false, Some(true)), 2)
        } else if ab(i, "eu") || ab(i, "äu") {
            (v("ɔø", "ɔø", false, Some(true)), 2)
        } else if ab(i, "au") {
            (v("aʊ", "aʊ", false, Some(true)), 2)
        } else if ab(i, "aa") {
            (v("aː", "aː", false, Some(true)), 2)
        } else if ab(i, "ee") {
            (v("eː", "eː", false, Some(true)), 2)
        } else if ab(i, "oo") {
            (v("oː", "oː", false, Some(true)), 2)
        } else if i + 1 < n && z[i] == z[i + 1] && !vokal_buchstabe(c) {
            // Doppelkonsonant: einmal sprechen, Vokal davor kurz.
            if let Some((
                Seg::Vokal {
                    kurz,
                    lang,
                    ist_e,
                    fest: None,
                },
                l,
            )) = segs.last().cloned()
            {
                *segs.last_mut().unwrap() = (
                    Seg::Vokal {
                        kurz,
                        lang,
                        ist_e,
                        fest: Some(false),
                    },
                    l,
                );
            }
            let seg = if c == 'r' {
                Seg::Kons("ʁ")
            } else {
                einzel_kons(z, i, true)
            };
            (seg, 2)
        } else {
            match c {
                'a' => (v("a", "aː", false, None), 1),
                'e' => (v("ɛ", "eː", true, None), 1),
                'i' => (v("ɪ", "iː", false, None), 1),
                'o' => (v("ɔ", "oː", false, None), 1),
                'u' => (v("ʊ", "uː", false, None), 1),
                'ä' => (v("ɛ", "ɛː", false, None), 1),
                'ö' => (v("œ", "øː", false, None), 1),
                'ü' => (v("ʏ", "yː", false, None), 1),
                'y' => {
                    // Am Wortanfang vor Vokal ein j („Yannick"), sonst ü.
                    if i == 0 && i + 1 < n && vokal_buchstabe(z[i + 1]) {
                        (Seg::Kons("j"), 1)
                    } else {
                        (v("ʏ", "yː", false, None), 1)
                    }
                }
                'h' => {
                    // Dehnungs-h: Vokal davor lang, selbst stumm.
                    let nach_vokal = matches!(segs.last(), Some((Seg::Vokal { .. }, _)));
                    let vor_vokal = i + 1 < n && vokal_buchstabe(z[i + 1]);
                    if nach_vokal && !vor_vokal {
                        if let Some((
                            Seg::Vokal {
                                kurz, lang, ist_e, ..
                            },
                            l,
                        )) = segs.last().cloned()
                        {
                            *segs.last_mut().unwrap() = (
                                Seg::Vokal {
                                    kurz,
                                    lang,
                                    ist_e,
                                    fest: Some(true),
                                },
                                l,
                            );
                        }
                        i += 1;
                        continue;
                    }
                    if !vor_vokal {
                        // „Minh", „Thanh": h nach Konsonant ohne Vokal danach.
                        i += 1;
                        continue;
                    }
                    (Seg::Kons("h"), 1)
                }
                'r' => (Seg::R, 1),
                _ => (einzel_kons(z, i, false), 1),
            }
        };
        if let Seg::Vokal { kurz, .. } = &seg {
            hinten = matches!(*kurz, "a" | "ɔ" | "ʊ" | "aʊ");
        }
        segs.push((seg, i));
        i += len;
    }
    segs
}

fn einzel_kons(z: &[char], i: usize, doppelt: bool) -> Seg {
    let n = z.len();
    let weiter = if doppelt { i + 2 } else { i + 1 };
    let naechster = z.get(weiter).copied();
    let vokal_folgt = naechster.is_some_and(|c| "aeiouäöüy".contains(c));
    // Auslautverhärtung: am Wortende oder vor Konsonant (außer l/r/n/j).
    let hart = weiter >= n || naechster.is_some_and(|c| !"aeiouäöüylrnj".contains(c));
    match z[i] {
        'b' => Seg::Kons(if hart { "p" } else { "b" }),
        'd' => Seg::Kons(if hart { "t" } else { "d" }),
        'g' => {
            if hart && i > 0 && z[i - 1] == 'i' && weiter >= n {
                Seg::Kons("ç")
            } else {
                Seg::Kons(if hart { "k" } else { "ɡ" })
            }
        }
        'c' => Seg::Kons(if naechster.is_some_and(|c| "eiyä".contains(c)) {
            "ts"
        } else {
            "k"
        }),
        'f' => Seg::Kons("f"),
        'j' => Seg::Kons("j"),
        'k' => Seg::Kons("k"),
        'l' => Seg::Kons("l"),
        'm' => Seg::Kons("m"),
        'n' => Seg::Kons("n"),
        'p' => Seg::Kons("p"),
        'q' => Seg::Kons("k"),
        's' => {
            if !doppelt && i == 0 && matches!(naechster, Some('p') | Some('t')) {
                Seg::Kons("ʃ")
            } else if !doppelt && vokal_folgt && (i == 0 || "aeiouäöüylrmn".contains(z[i - 1])) {
                Seg::Kons("z")
            } else {
                Seg::Kons("s")
            }
        }
        'ß' => Seg::Kons("s"),
        't' => Seg::Kons("t"),
        'v' => Seg::Kons("f"),
        'w' => Seg::Kons("v"),
        'x' => Seg::Kons("ks"),
        'z' => Seg::Kons("ts"),
        // Unbekannter Buchstabe: stumm statt falsch.
        _ => Seg::Kons(""),
    }
}

fn aussprechen(z: &[char], segs: &[(Seg, usize)]) -> String {
    // Silbenkerne finden.
    let kerne: Vec<usize> = segs
        .iter()
        .enumerate()
        .filter(|(_, (s, _))| matches!(s, Seg::Vokal { .. }))
        .map(|(i, _)| i)
        .collect();
    if kerne.is_empty() {
        return String::new();
    }
    // Betonung: erste Silbe. Der Schätzer liest fast nur Namen; die
    // Regel der unbetonten Vorsilben (be-, ge-, ver-) machte aus „Berger"
    // und „Gerhardt" Fehlbetonungen, gängige Wörter stehen im Lexikon.
    let betont = kerne[0];
    let mut aus = String::new();
    for (k, (seg, _)) in segs.iter().enumerate() {
        match seg {
            Seg::Kons(l) => aus.push_str(l),
            Seg::R => {
                let nach_vokal = k > 0 && matches!(segs[k - 1].0, Seg::Vokal { .. });
                let vor_vokal = matches!(segs.get(k + 1), Some((Seg::Vokal { .. }, _)));
                let wortende = k + 1 == segs.len();
                // Nach dem betonten Vokal bleibt r im Wortinneren ein r
                // („Berger"), sonst wird es vokalisch („Müller", „Weber").
                if nach_vokal && !vor_vokal && (wortende || k - 1 != betont) {
                    // „er" unbetont: das Schwa geht im ɐ auf.
                    if aus.ends_with('ə') {
                        aus.pop();
                    }
                    aus.push('ɐ');
                } else {
                    aus.push('ʁ');
                }
            }
            Seg::Vokal {
                kurz,
                lang,
                ist_e,
                fest,
            } => {
                if k == betont {
                    aus.push('ˈ');
                }
                // Nur die betonte Silbe wird durch offene Silbe lang;
                // unbetonte Vokale bleiben kurz („Harrison").
                let laenge = fest.unwrap_or_else(|| k == betont && offene_silbe(segs, k, z.len()));
                let laut = if *ist_e && k > betont {
                    "ə"
                } else if laenge {
                    lang
                } else {
                    kurz
                };
                aus.push_str(laut);
            }
        }
    }
    betonung_vor_vokal(&aus)
}

/// Lang, wenn höchstens ein Konsonant bis zum nächsten Vokal folgt.
/// Mehrbuchstabige Konsonanten (sch, ck, tz, ch) schließen die Silbe.
fn offene_silbe(segs: &[(Seg, usize)], k: usize, wortlaenge: usize) -> bool {
    let mut konsonanten = 0;
    for (j, (s, start)) in segs.iter().enumerate().skip(k + 1) {
        let ende = segs.get(j + 1).map(|x| x.1).unwrap_or(wortlaenge);
        match s {
            Seg::Vokal { .. } => return konsonanten <= 1,
            Seg::Kons(l) if l.chars().count() > 1 || ende - start > 1 => konsonanten += 2,
            Seg::Kons(_) | Seg::R => konsonanten += 1,
        }
    }
    // Wortende: kein Konsonant danach = lang („Bu"), sonst kurz.
    konsonanten == 0
}

// --- Zahlen ------------------------------------------------------------------

/// Ziffernfolge Ziffer für Ziffer („007" → „null null sieben").
fn ziffernweise(ziffern: &str) -> String {
    ziffern
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| zahlwort(u64::from(d)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Zahl als deutsches Wort, Bestandteile mit Leerzeichen getrennt
/// („einundzwanzig" → „ein und zwanzig"), damit das Lexikon sie findet.
/// Über 999.999 wird Ziffer für Ziffer gesprochen.
pub fn zahlwort(n: u64) -> String {
    const EINER: [&str; 20] = [
        "null",
        "eins",
        "zwei",
        "drei",
        "vier",
        "fünf",
        "sechs",
        "sieben",
        "acht",
        "neun",
        "zehn",
        "elf",
        "zwölf",
        "dreizehn",
        "vierzehn",
        "fünfzehn",
        "sechzehn",
        "siebzehn",
        "achtzehn",
        "neunzehn",
    ];
    const ZEHNER: [&str; 10] = [
        "", "", "zwanzig", "dreißig", "vierzig", "fünfzig", "sechzig", "siebzig", "achtzig",
        "neunzig",
    ];
    fn unter_hundert(n: u64, aus: &mut Vec<String>) {
        if n < 20 {
            aus.push(EINER[n as usize].to_string());
        } else {
            let e = n % 10;
            if e > 0 {
                aus.push(if e == 1 { "ein" } else { EINER[e as usize] }.to_string());
                aus.push("und".to_string());
            }
            aus.push(ZEHNER[(n / 10) as usize].to_string());
        }
    }
    fn unter_tausend(n: u64, aus: &mut Vec<String>) {
        let h = n / 100;
        if h > 0 {
            aus.push(if h == 1 { "ein" } else { EINER[h as usize] }.to_string());
            aus.push("hundert".to_string());
        }
        let r = n % 100;
        if r > 0 || h == 0 {
            unter_hundert(r, aus);
        }
    }
    if n > 999_999 {
        return n
            .to_string()
            .chars()
            .map(|c| EINER[c.to_digit(10).unwrap_or(0) as usize])
            .collect::<Vec<_>>()
            .join(" ");
    }
    let mut aus = Vec::new();
    let t = n / 1000;
    if t > 0 {
        if t == 1 {
            aus.push("ein".to_string());
        } else {
            unter_tausend(t, &mut aus);
            if aus.last().is_some_and(|w| w == "eins") {
                *aus.last_mut().unwrap() = "ein".to_string();
            }
        }
        aus.push("tausend".to_string());
    }
    let r = n % 1000;
    if r > 0 || t == 0 {
        unter_tausend(r, &mut aus);
    }
    aus.join(" ")
}

#[cfg(test)]
#[path = "laute_tests.rs"]
mod tests;
