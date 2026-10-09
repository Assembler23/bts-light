// Benennt eine Disziplin — bei Jugendklassen als Jungen-/Mädchen-Disziplin.
//
// BTP kennt nur Herren/Damen/Mixed (`GenderID`), auch für Jugendturniere.
// Ein DBV-Jugendturnier (10/2026) nennt seine Klassen „JE U15", „MD U17" —
// dort hieße es sonst überall „Herreneinzel". Erkannt wird die Jugend an der
// Altersklasse „U" + Zahl, die `class_label` (btp/model.rs) aus dem
// Event-Namen liest.
//
// Inline-Kopien in `src-tauri/assets/tl.html` und `monitor.html` —
// `scripts/test-disziplin-name.mjs` hält sie wortgleich. Deshalb ES5-Stil
// und nur doppelte Anführungszeichen.
export function disziplinName(key, klasse) {
  var jugend = /^U\d{1,2}$/i.test(String(klasse || "").trim());
  var t = {
    mens_singles: ["HE", "Herreneinzel", "Men's Singles", "JE", "Jungeneinzel", "Boys' Singles"],
    womens_singles: ["DE", "Dameneinzel", "Women's Singles", "ME", "Mädcheneinzel", "Girls' Singles"],
    mens_doubles: ["HD", "Herrendoppel", "Men's Doubles", "JD", "Jungendoppel", "Boys' Doubles"],
    womens_doubles: ["DD", "Damendoppel", "Women's Doubles", "MD", "Mädchendoppel", "Girls' Doubles"],
    mixed: ["GD", "Mixed", "Mixed", "MX", "Mixed", "Mixed"]
  }[key];
  if (!t) return { kurz: "", de: "", en: "" };
  var o = jugend ? 3 : 0;
  return { kurz: t[o], de: t[o + 1], en: t[o + 2] };
}
