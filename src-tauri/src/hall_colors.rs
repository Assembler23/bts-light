//! Hallen-Farben (Spec `docs/features/hallen-farben.md`, ADR 0031–0033).
//!
//! Jede Halle eines Mehr-Hallen-Turniers bekommt eine Farbe: automatisch aus
//! der kuratierten Palette, deterministisch über die alphabetisch sortierte
//! Hallenliste (ADR 0032), übersteuerbar je Halle in der Config (ADR 0031).
//! Auf dem Draht reist immer der Hex-Wert selbst (ADR 0033) — Konsumenten
//! brauchen keinen Paletten-Spiegel.

use crate::config::AppConfig;

/// Die kuratierte Palette: ~10 Töne, als kleine Farbmarke auf hellem UND
/// dunklem Grund erkennbar. Bewusst ausgespart sind die Farbtonbereiche der
/// Feld-Zustandsfarben (Rot = überfällig, Grün = läuft, Violett = beendet),
/// damit eine Hallen-Marke nie wie ein Feldzustand liest. Die ersten Töne
/// sind maximal unterscheidbar — real haben Turniere 2–4 Hallen.
pub const HALL_PALETTE: [&str; 10] = [
    "#f59e0b", // Bernstein
    "#0ea5e9", // Himmelblau
    "#ec4899", // Pink
    "#14b8a6", // Türkis
    "#f97316", // Orange
    "#2563eb", // Blau
    "#eab308", // Gelb
    "#a16207", // Ocker
    "#06b6d4", // Cyan
    "#64748b", // Schiefer
];

/// Effektive Farbe je Halle: persistierte Übersteuerung gewinnt, sonst
/// Auto-Palette über die getrimmte, case-insensitiv alphabetisch sortierte
/// Hallenliste (ADR 0032). Bei weniger als zwei Hallen leer — das Feature
/// ist bei Ein-Hallen-Turnieren strukturell unsichtbar.
///
/// `halls` darf ungeordnet sein und Dubletten/Umbrüche in Schreibweise
/// tragen — die Rückgabe nennt jede Halle genau einmal (erste gesehene
/// Schreibweise, getrimmt) mit ihrem Hex-Ton.
pub fn effective_hall_colors(cfg: &AppConfig, halls: &[String]) -> Vec<(String, String)> {
    // Dedup + Sortierung über den lowercase-Schlüssel, damit Schreibweise
    // und Snapshot-Reihenfolge keine Rolle spielen (ADR 0032).
    let mut gesehen: Vec<(String, String)> = Vec::new(); // (schlüssel, anzeige)
    for h in halls {
        let anzeige = h.trim();
        if anzeige.is_empty() {
            continue;
        }
        let schluessel = anzeige.to_lowercase();
        if !gesehen.iter().any(|(s, _)| *s == schluessel) {
            gesehen.push((schluessel, anzeige.to_string()));
        }
    }
    if gesehen.len() < 2 {
        return Vec::new();
    }
    gesehen.sort_by(|a, b| a.0.cmp(&b.0));
    gesehen
        .into_iter()
        .enumerate()
        .map(|(i, (schluessel, anzeige))| {
            let farbe = cfg
                .hall_colors
                .iter()
                .find(|c| c.hall.trim().to_lowercase() == schluessel)
                .map(|c| c.color.clone())
                .unwrap_or_else(|| HALL_PALETTE[i % HALL_PALETTE.len()].to_string());
            (anzeige, farbe)
        })
        .collect()
}

/// Farbe EINER Halle (getrimmter, case-insensitiver Abgleich) — `None` bei
/// Ein-Hallen-Turnieren oder unbekannter Halle.
pub fn color_for(cfg: &AppConfig, halls: &[String], hall: &str) -> Option<String> {
    let gesucht = hall.trim().to_lowercase();
    effective_hall_colors(cfg, halls)
        .into_iter()
        .find(|(h, _)| h.to_lowercase() == gesucht)
        .map(|(_, farbe)| farbe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn halls(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn auto_palette_is_assigned_alphabetically_regardless_of_snapshot_order() {
        // Der BTP-Snapshot darf die Hallen in beliebiger Reihenfolge nennen —
        // die Farben müssen trotzdem stabil sein (ADR 0032), sonst springen
        // sie mitten im Turnier beim Neuladen.
        let cfg = AppConfig::default();
        let a = effective_hall_colors(&cfg, &halls(&["Nord", "Mitte", "Süd"]));
        let b = effective_hall_colors(&cfg, &halls(&["Süd", "Nord", "Mitte"]));
        assert_eq!(a, b, "Reihenfolge im Snapshot ist egal");
        assert_eq!(a[0].0, "Mitte");
        assert_eq!(a[0].1, HALL_PALETTE[0]);
        assert_eq!(a[1].0, "Nord");
        assert_eq!(a[1].1, HALL_PALETTE[1]);
        assert_eq!(a[2].0, "Süd");
        assert_eq!(a[2].1, HALL_PALETTE[2]);
    }

    #[test]
    fn effective_colors_prefer_the_persisted_override() {
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_color("Nord", HALL_PALETTE[7]).unwrap();
        let farben = effective_hall_colors(&cfg, &halls(&["Mitte", "Nord"]));
        assert_eq!(
            farben[0],
            ("Mitte".to_string(), HALL_PALETTE[0].to_string())
        );
        assert_eq!(
            farben[1],
            ("Nord".to_string(), HALL_PALETTE[7].to_string()),
            "die Übersteuerung gewinnt"
        );
    }

    #[test]
    fn an_override_does_not_reshuffle_the_auto_colors_of_other_halls() {
        // ADR 0032: fremde Übersteuerungen sortieren die Auto-Vergabe NICHT
        // um — sonst wechselte Halle „Süd" die Farbe, nur weil „Nord" eine
        // Wunschfarbe bekam.
        let mut cfg = AppConfig::default();
        let vorher = effective_hall_colors(&cfg, &halls(&["Mitte", "Nord", "Süd"]));
        cfg.upsert_hall_color("Nord", HALL_PALETTE[9]).unwrap();
        let nachher = effective_hall_colors(&cfg, &halls(&["Mitte", "Nord", "Süd"]));
        assert_eq!(vorher[0], nachher[0], "Mitte unverändert");
        assert_eq!(vorher[2], nachher[2], "Süd unverändert");
    }

    #[test]
    fn single_hall_tournament_gets_no_colors() {
        let cfg = AppConfig::default();
        assert!(effective_hall_colors(&cfg, &halls(&["Einzige"])).is_empty());
        assert!(effective_hall_colors(&cfg, &[]).is_empty());
        assert_eq!(color_for(&cfg, &halls(&["Einzige"]), "Einzige"), None);
    }

    #[test]
    fn halls_are_deduplicated_case_insensitive_before_assignment() {
        // Courts liefern die Halle je Feld — dieselbe Halle taucht also
        // mehrfach und ggf. in wechselnder Schreibweise auf.
        let cfg = AppConfig::default();
        let farben = effective_hall_colors(&cfg, &halls(&["Nord", " nord ", "Mitte", "NORD"]));
        assert_eq!(farben.len(), 2, "eine Zeile je Halle");
        assert_eq!(farben[0].0, "Mitte");
        assert_eq!(farben[1].0, "Nord", "erste gesehene Schreibweise, getrimmt");
    }

    #[test]
    fn color_for_matches_trimmed_case_insensitive() {
        let cfg = AppConfig::default();
        let liste = halls(&["Nord", "Mitte"]);
        assert_eq!(
            color_for(&cfg, &liste, "  MITTE "),
            Some(HALL_PALETTE[0].to_string())
        );
        assert_eq!(color_for(&cfg, &liste, "Unbekannt"), None);
    }

    #[test]
    fn more_halls_than_palette_tones_wrap_around() {
        let cfg = AppConfig::default();
        let viele: Vec<String> = (0..12).map(|i| format!("Halle {i:02}")).collect();
        let farben = effective_hall_colors(&cfg, &viele);
        assert_eq!(farben.len(), 12);
        assert_eq!(farben[10].1, HALL_PALETTE[0], "elfte Halle beginnt vorn");
    }

    #[test]
    fn palette_avoids_state_color_hues() {
        // Struktureller Wächter: kein Palettenton darf im Farbtonbereich der
        // Feld-Zustandsfarben liegen (Rot = überfällig, Grün = läuft,
        // Violett = beendet) — sonst liest eine Hallen-Marke wie ein Zustand.
        for hex in HALL_PALETTE {
            let (r, g, b) = (
                u8::from_str_radix(&hex[1..3], 16).unwrap() as f32,
                u8::from_str_radix(&hex[3..5], 16).unwrap() as f32,
                u8::from_str_radix(&hex[5..7], 16).unwrap() as f32,
            );
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let d = max - min;
            assert!(d > 0.0, "{hex}: Grau wäre keine erkennbare Marke");
            let h = if max == r {
                60.0 * (((g - b) / d) % 6.0)
            } else if max == g {
                60.0 * ((b - r) / d + 2.0)
            } else {
                60.0 * ((r - g) / d + 4.0)
            };
            let h = if h < 0.0 { h + 360.0 } else { h };
            assert!(
                !(h < 15.0 || h > 345.0),
                "{hex} (h={h:.0}) liegt im Rot-Bereich"
            );
            assert!(
                !(100.0..=160.0).contains(&h),
                "{hex} (h={h:.0}) liegt im Grün-Bereich"
            );
            assert!(
                !(250.0..=310.0).contains(&h),
                "{hex} (h={h:.0}) liegt im Violett-Bereich"
            );
        }
    }

    #[test]
    fn palette_tones_are_unique_and_lowercase_hex() {
        let mut gesehen = std::collections::HashSet::new();
        for hex in HALL_PALETTE {
            assert!(gesehen.insert(hex), "{hex} doppelt in der Palette");
            assert!(
                hex.len() == 7
                    && hex.starts_with('#')
                    && hex[1..].chars().all(|c| c.is_ascii_hexdigit())
                    && hex == hex.to_lowercase(),
                "{hex}: erwartet lowercase #rrggbb"
            );
        }
    }
}
