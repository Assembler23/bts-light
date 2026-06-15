// Vorbefüllte Aussprache-Korrekturen für häufige vietnamesische, chinesische
// und indische Nachnamen. Über den Knopf „Häufige Namen laden" in den
// Ansage-Einstellungen einfügbar.
//
// WICHTIG: Das sind **editierbare Startwerte**, keine perfekte Lautschrift.
// Sie zielen auf die **deutsche** TTS-Stimme (deutsche Lese-Regeln) und beheben
// vor allem Buchstabenfolgen, die eine deutsche Stimme zuverlässig falsch liest:
//   ph→f · tr→tsch · th→t · x→sch · zh→dsch · q→tsch · c(e/i/a)→ts · sh→sch.
// Vietnamesisch ist tonal — eine Schriftnäherung kann den Ton nicht abbilden;
// „besser als die deutsche Default-Lesung" ist das Ziel. Mit dem Test-Knopf je
// Zeile nach Gehör feinjustieren.

import type { NameOverride } from "../types";

export const COMMON_NAME_OVERRIDES: NameOverride[] = [
  // ── Vietnamesisch ────────────────────────────────────────────────────
  { name: "Nguyen", say: "Nujen" },
  { name: "Tran", say: "Tschan" },
  { name: "Pham", say: "Fam" },
  { name: "Phan", say: "Fan" },
  { name: "Le", say: "Leh" },
  { name: "Hoang", say: "Huang" },
  { name: "Huynh", say: "Hwinj" },
  { name: "Vo", say: "Wo" },
  { name: "Dang", say: "Dang" },
  { name: "Bui", say: "Bui" },
  { name: "Truong", say: "Tschuong" },
  { name: "Duong", say: "Juong" },
  { name: "Ngo", say: "Ngo" },
  { name: "Ly", say: "Li" },
  { name: "Xuan", say: "Suan" },
  { name: "Thi", say: "Ti" },
  { name: "Thanh", say: "Tan" },

  // ── Chinesisch (Pinyin → deutsche Näherung) ──────────────────────────
  { name: "Zhang", say: "Dschang" },
  { name: "Zhao", say: "Dschao" },
  { name: "Zhou", say: "Dschou" },
  { name: "Zheng", say: "Dscheng" },
  { name: "Xu", say: "Schü" },
  { name: "Xie", say: "Schieh" },
  { name: "Xia", say: "Schia" },
  { name: "Qin", say: "Tschin" },
  { name: "Qiu", say: "Tschiu" },
  { name: "Cai", say: "Tsai" },
  { name: "Cao", say: "Tsao" },
  { name: "Wang", say: "Uang" },
  { name: "Wu", say: "Uu" },
  { name: "Huang", say: "Huang" },
  { name: "Chen", say: "Tschen" },
  { name: "Jiang", say: "Dschiang" },

  // ── Indisch ──────────────────────────────────────────────────────────
  { name: "Sharma", say: "Scharma" },
  { name: "Shah", say: "Scha" },
  { name: "Singh", say: "Sing" },
  { name: "Reddy", say: "Reddi" },
  { name: "Krishnan", say: "Krischnan" },
  { name: "Chandra", say: "Tschandra" },
  { name: "Iyer", say: "Aijer" },
  { name: "Nair", say: "Näär" },
  { name: "Chowdhury", say: "Tschaudhuri" },
  { name: "Acharya", say: "Atscharja" },
];
