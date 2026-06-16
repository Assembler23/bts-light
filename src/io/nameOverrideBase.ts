// Mitgeliefertes **Basis-Wörterbuch** für die Aussprache-Korrektur der Ansage.
// Wird automatisch angewendet (kein „Laden" nötig); die Nutzer-Tabelle
// (`AnnounceConfig.name_overrides`) hat Vorrang vor diesen Einträgen.
//
// Inhalt: häufige **fremdsprachige** Nachnamen, die eine **deutsche** TTS-Stimme
// zuverlässig falsch liest — abgeleitet aus den häufigsten Nachnamen der
// Badhub-Spieler:innen-DB (Frequenz-Top, Stand 2026-06). Deutschsprachige Namen
// (Müller, Schmidt, …) stehen bewusst NICHT drin: die Stimme liest sie korrekt,
// ein Override würde sie verschlechtern.
//
// WICHTIG:
// - Schlüssel (`name`) in **ASCII-Grundform** — das Matching faltet Diakritika
//   und Sonderbuchstaben (ê→e, ı→i, ş→s, ç→c, ğ→g, ł→l, ø→o …), sodass z. B.
//   „Nguyên", „Nguyen", „NGUYEN" alle denselben Eintrag treffen.
// - Kurze Einträge (z. B. „Le", „Do", „Vu", „Wu") greifen Wort-für-Wort, also
//   auch als Bestandteil längerer/zusammengesetzter Namen. Ein versehentlicher
//   Treffer lässt sich mit einem Nutzer-Eintrag (Vorrang) überschreiben, dessen
//   `say` = Originalschreibweise ist.
// - `say` = deutsche Lautschrift-Näherung. Manche Sprachen (tonales
//   Vietnamesisch, Mandarin-Töne) lassen sich nur näherungsweise abbilden —
//   „besser als die Default-Lesung" ist das Ziel. Korrekturen pflegt der Nutzer
//   in der Tabelle (Vorrang) mit dem ▶-Test.

import type { NameOverride } from "../types";

export const BASE_NAME_OVERRIDES: NameOverride[] = [
  // ── Vietnamesisch ────────────────────────────────────────────────────
  { name: "Nguyen", say: "Nujen" },
  { name: "Tran", say: "Tschan" },
  { name: "Le", say: "Leh" },
  { name: "Pham", say: "Fam" },
  { name: "Phan", say: "Fan" },
  { name: "Vu", say: "Wu" },
  { name: "Vo", say: "Wo" },
  { name: "Do", say: "Doh" },
  { name: "Truong", say: "Tschuong" },
  { name: "Hoang", say: "Huang" },
  { name: "Duong", say: "Juong" },
  { name: "Phung", say: "Fung" },
  { name: "Ngo", say: "Ngoh" },
  { name: "Ly", say: "Li" },
  { name: "Dinh", say: "Dinj" },
  { name: "Huynh", say: "Hwinj" },
  { name: "Luong", say: "Luong" },

  // ── Chinesisch (Pinyin → deutsche Näherung) ──────────────────────────
  { name: "Wang", say: "Uang" },
  { name: "Chen", say: "Tschen" },
  { name: "Zhang", say: "Dschang" },
  { name: "Liu", say: "Liou" },
  { name: "Wu", say: "Uu" },
  { name: "Xu", say: "Schü" },
  { name: "Zhu", say: "Dschu" },
  { name: "Zhou", say: "Dschou" },
  { name: "Zhao", say: "Dschao" },
  { name: "Jiang", say: "Dschiang" },
  { name: "Zheng", say: "Dscheng" },
  { name: "Cheng", say: "Tscheng" },
  { name: "Jin", say: "Dschin" },
  { name: "Shen", say: "Schen" },
  { name: "Guo", say: "Gwo" },
  { name: "Luo", say: "Lwo" },
  { name: "Wong", say: "Uong" },
  { name: "Shi", say: "Schi" },
  { name: "Chan", say: "Tschan" },
  { name: "Chang", say: "Tschang" },
  { name: "Choi", say: "Tschoi" },
  { name: "Qu", say: "Tschü" },
  { name: "Ji", say: "Dschi" },
  { name: "Zhong", say: "Dschong" },
  { name: "Xie", say: "Schieh" },
  { name: "Xia", say: "Schia" },
  { name: "Qin", say: "Tschin" },
  { name: "Qiu", say: "Tschiu" },
  { name: "Cai", say: "Tsai" },
  { name: "Cao", say: "Tsao" },
  { name: "Sun", say: "Ssun" },
  { name: "Wei", say: "Uei" },

  // ── Indisch ──────────────────────────────────────────────────────────
  { name: "Sharma", say: "Scharma" },
  { name: "Shah", say: "Scha" },
  { name: "Singh", say: "Sing" },
  { name: "Reddy", say: "Reddi" },
  { name: "Krishnan", say: "Krischnan" },
  { name: "Chandra", say: "Tschandra" },
  { name: "Iyer", say: "Aijer" },
  { name: "Nair", say: "Näär" },
  { name: "Jain", say: "Dschain" },
  { name: "Acharya", say: "Atscharja" },

  // ── Türkisch (ç→tsch, ş→sch, ğ stumm, y→j, z→s) ──────────────────────
  { name: "Yilmaz", say: "Jilmas" },
  { name: "Yildiz", say: "Jildis" },
  { name: "Yildirim", say: "Jildirim" },
  { name: "Sahin", say: "Schahin" },
  { name: "Celik", say: "Tschelik" },
  { name: "Cetin", say: "Tschetin" },
  { name: "Ozturk", say: "Östürk" },
  { name: "Dogan", say: "Doan" },
  { name: "Kilic", say: "Kilitsch" },
  { name: "Koc", say: "Kotsch" },
  { name: "Ozdemir", say: "Ösdemir" },
  { name: "Simsek", say: "Schimschek" },
  { name: "Gunes", say: "Günesch" },
  { name: "Coban", say: "Tschoban" },

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

  // ── Spanisch (j→ch, ll→j, ñ→nj, z/ce/ci→s, h stumm) ──────────────────
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

  // ── Polnisch (sz→sch, cz→tsch, rz→sch, ł→w) ──────────────────────────
  { name: "Szymański", say: "Schimanski" },
  { name: "Wójcik", say: "Wuitschik" },
  { name: "Wiśniewski", say: "Wischnjewski" },
  { name: "Kaczmarek", say: "Katschmarek" },
  { name: "Krawczyk", say: "Krawtschik" },

  // ── Vornamen (international) ──────────────────────────────────────────
  // Hinweis: Badhubs HÄUFIGE Vornamen sind überwiegend deutsch (werden korrekt
  // gelesen). Fremdsprachige Vornamen sind ein verstreuter Long-Tail — hier die
  // GÄNGIGEN, die eine deutsche Stimme klar falsch liest. Kollisionsträchtige
  // kurze Token (z. B. „Van" → bräche niederländisches „van der …") bewusst
  // weggelassen. Auch hier: Näherungen, in der Nutzer-Tabelle korrigierbar.
  // Vietnamesisch:
  { name: "Anh", say: "An" },
  { name: "Minh", say: "Minj" },
  { name: "Duc", say: "Dück" },
  { name: "Quang", say: "Kwang" },
  { name: "Huy", say: "Hui" },
  { name: "Thi", say: "Ti" },
  { name: "Thanh", say: "Tan" },
  { name: "Linh", say: "Linj" },
  { name: "Huong", say: "Hwong" },
  // Chinesisch:
  { name: "Jun", say: "Dschün" },
  { name: "Yi", say: "I" },
  { name: "Xin", say: "Schin" },
  { name: "Jing", say: "Dsching" },
  { name: "Jie", say: "Dschieh" },
  { name: "Qiang", say: "Tschiang" },
  // Indisch:
  { name: "Arjun", say: "Ardschun" },
  { name: "Priya", say: "Prija" },
  { name: "Raj", say: "Radsch" },
  { name: "Vijay", say: "Widschej" },
  { name: "Sanjay", say: "Sandschej" },
  { name: "Aditya", say: "Aditja" },
  { name: "Youssef", say: "Jussef" },
  // Türkisch:
  { name: "Can", say: "Dschan" },
  { name: "Cem", say: "Dschem" },
  { name: "Deniz", say: "Denis" },
  { name: "Oguz", say: "Ous" },
  { name: "Cagri", say: "Tschari" },
];
