// Vorbefüllte Aussprache-Korrekturen für häufige Nachnamen vieler Herkünfte
// (vietnamesisch, chinesisch, indisch, französisch, spanisch, türkisch,
// polnisch). Über den Knopf „Häufige Namen laden" in den Ansage-Einstellungen
// einfügbar.
//
// WICHTIG: Das sind **editierbare Startwerte**, keine perfekte Lautschrift.
// Sie zielen auf die **deutsche** TTS-Stimme (deutsche Lese-Regeln) und beheben
// Buchstabenfolgen, die eine deutsche Stimme zuverlässig falsch liest, u. a.:
//   ph→f · tr→tsch · th→t · x→sch · zh→dsch · q→tsch · sh→sch (asiatisch/indisch);
//   stille Endungen · j→sch · Nasale (französisch); j→ch · ll→j · ñ→nj · z→s
//   (spanisch); ç→tsch · ş→sch · ğ stumm · y→j (türkisch); sz→sch · cz→tsch ·
//   ł→w (polnisch).
// Manche Sprachen (z. B. tonales Vietnamesisch) lassen sich nur näherungsweise
// in Schrift abbilden; „besser als die deutsche Default-Lesung" ist das Ziel.
// Mit dem Test-Knopf je Zeile nach Gehör feinjustieren.

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

  // ── Französisch (stille Endungen, j→sch, ch→sch, Nasale) ─────────────
  { name: "Lefebvre", say: "Löfäwr" },
  { name: "Rousseau", say: "Russo" },
  { name: "Moreau", say: "Moro" },
  { name: "Petit", say: "Pöti" },
  { name: "Dupont", say: "Düpong" },
  { name: "Durand", say: "Dürang" },
  { name: "Leroy", say: "Lörua" },
  { name: "Girard", say: "Schirar" },
  { name: "Chevalier", say: "Schöwalje" },
  { name: "Gauthier", say: "Gotje" },
  { name: "Mercier", say: "Mersje" },
  { name: "Fontaine", say: "Fongtähn" },
  { name: "Dubois", say: "Dübua" },
  { name: "Lemoine", say: "Lömuan" },
  { name: "Renault", say: "Reno" },

  // ── Spanisch (j→ch, ll→j, ñ→nj, z/ce/ci→s, h stumm, v→b) ──────────────
  { name: "García", say: "Garsia" },
  { name: "Jiménez", say: "Chiménes" },
  { name: "González", say: "Gonsales" },
  { name: "Sánchez", say: "Santschés" },
  { name: "Vázquez", say: "Báskes" },
  { name: "López", say: "Lopes" },
  { name: "Hernández", say: "Ernándes" },
  { name: "Rodríguez", say: "Rodriges" },
  { name: "Muñoz", say: "Munjos" },
  { name: "Jorge", say: "Chorche" },
  { name: "José", say: "Chosé" },
  { name: "Cabrera", say: "Kabrera" },

  // ── Türkisch (c→dsch, ç→tsch, ş→sch, ğ stumm, y→j, z→s) ──────────────
  { name: "Yılmaz", say: "Jilmas" },
  { name: "Şahin", say: "Schahin" },
  { name: "Çelik", say: "Tschelik" },
  { name: "Çetin", say: "Tschetin" },
  { name: "Yıldız", say: "Jildis" },
  { name: "Yıldırım", say: "Jildirim" },
  { name: "Öztürk", say: "Östürk" },
  { name: "Doğan", say: "Doan" },
  { name: "Kılıç", say: "Kilitsch" },
  { name: "Koç", say: "Kotsch" },
  { name: "Özdemir", say: "Ösdemir" },
  { name: "Şimşek", say: "Schimschek" },
  { name: "Güneş", say: "Günesch" },

  // ── Polnisch (sz→sch, cz→tsch, rz→sch, ł→w, ó→u) ─────────────────────
  { name: "Szymański", say: "Schimanski" },
  { name: "Wójcik", say: "Wuitschik" },
  { name: "Wiśniewski", say: "Wischnjewski" },
  { name: "Kaczmarek", say: "Katschmarek" },
  { name: "Woźniak", say: "Wosniak" },
  { name: "Krawczyk", say: "Krawtschik" },
];
