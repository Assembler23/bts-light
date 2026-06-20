# Umsetzungsplan: Namensaussprache via Azure — `<lang>` nativ als Workhorse

> **Status: freigegebener Plan (zur Prüfung), noch nicht implementiert.**
> Ergänzt/aktualisiert das Konzept in [tts-azure-concept.md](../tts-azure-concept.md) und
> die Implementierung in [announcements.md](../announcements.md).

## Ziel & Kontext
Deutsche Hallen-Ansage über **Azure TTS in bester Qualität** — Namen möglichst
**originalgetreu**. Sorge war, dass die Datenmenge zu groß wird, um „alles" pro Turnier zu laden.

**Schwenk (nach Review von `tts-azure-concept.md`):** weg von „IPA-`<phoneme>` mit großen
kuratierten Lexika" (Datenlast, Pflege, nur Näherung auf de-Stimme) **hin zu `<lang>` nativ** —
die mehrsprachige Azure-Stimme spricht jeden Namen in **seiner erkannten Sprache** (echte
Töne/Laute). `<lang>` braucht **keine Pro-Name-Lexika**, nur eine gute **Sprach-Klassifikation**
(winzige, mitgelieferte Daten) → die Datenmengen-Sorge ist damit erledigt. Das Nadelöhr wandert
von „Lautqualität" zu „korrekter Klassifikation".

`<lang>` existiert bereits als Fallback (`nameSsml` in `src/io/announcer.ts`: `<phoneme>` →
`<lang>` via `detectNameLang`, heute nur zh/vn → roh). IPA-`<phoneme>` (v0.9.138) bleibt als
**Ausnahme-Tier** erhalten, nicht verworfen.

## Entscheidungen
- **Primär = `<lang>` nativ** (Workhorse). Klassifikation auf **es/fr/pl/tr/ms/in** erweitern;
  kuratiertes IPA-`<phoneme>` nur noch als **Ausnahme**, wo `<lang>` schwächelt.
  **Spike entscheidet final pro Sprache.**
- **Datenschutz: dokumentieren** (opt-in `enabled=false`, EU-Region West Europe, nur Namen).
- **Datenmenge**: gelöst durch `<lang>` — Klassifikations-Daten **mitgeliefert** (bundled, klein);
  kein Pro-Turnier-Netz-Lookup, kein großes Runtime-Lexikon.

## Wiederverwendung / was bleibt
- `detectNameLang` + CN/VN-Surname-Listen (`src/io/transliterate.ts`) — Basis der Klassifikation.
- Kuratierte Lexika (`db/seed/*.xml`, de + 6 Sprachen) **umfunktioniert zu Name→Sprache-Listen**
  (Dateiname = Locale; Grapheme = sichere Treffer) → mitgeliefertes `nameLangBase.ts`.
- IPA-`<phoneme>`-Tier (v0.9.138) für **Einzelfälle**; `tts_pronunciations`-DB/Community bleibt für
  den **Web-Speech-`say`-Fallback** + IPA-Ausnahmen.
- `azure_tts.rs` (Synthese + Cache), Web-Speech-Fallback (`announcer.ts`) — robust, unverändert.

## Phase 0 — Spike (Entscheidungs-Gate, braucht Azure-Key) — ZUERST
2–3 mehrsprachige Stimmen (z. B. Seraphina/Florian Multilingual) mit **einem gemischten SSML**
gegenhören, **harte Testfälle**:
- Vietnamesisch mit **Tönen**: „Nguyễn Thị Hồng", „Phạm Thị Hồng Thu" (nicht nur tonarme wie „Pham").
- Mandarin: „Zhang Zhixin", „Xu Yinsong". Europäisch: „García", „Lefèvre", „Wiśniewski", „Yılmaz".
- Pro Sprache vergleichen: `<lang>` vs. `<phoneme>` (wo IPA da) vs. roh.

**Output:** Go/No-Go für `<lang>`-primär + Stimmenwahl + **Verdikt je Sprache** (wo `<lang>` reicht,
wo IPA-Ausnahme nötig, wo lieber de-Default).

## Phase 1 — Klassifikation als Kern (`detectNameLang` ausbauen + Konfidenz)
- `detectNameLang` über zh/vn hinaus auf **es/fr/pl/tr/ms/in** erweitern:
  - **Name→Sprache-Listen** aus den Lexika (neues, mitgeliefertes `src/io/nameLangBase.ts`,
    generiert aus `db/seed/*.xml`) = sichere Treffer (hohe Konfidenz).
  - **Diakritika-Signale** (ł/ż/ś→pl, ı/ğ/ş→tr, ñ→es) + **Morphologie** (-ski/-czyk→pl, -oğlu→tr,
    -ez→es) als Wahrscheinlichkeit.
  - Locale-Mapping: zh-CN, vi-VN, es-ES, fr-FR, pl-PL, tr-TR, ms-MY, en-IN.
- **Konfidenz-Modell:** hoch → `<lang>` setzen; **niedrig/mehrdeutig** („Le", „Kim", „Lee",
  „Martin") → **kein `<lang>`** → de-Default (neutraler Rückfall statt souverän-falsch).
- `nameSsml`-Vorrang: kuratiertes IPA-`<phoneme>` (Ausnahme) → `<lang>` (sicher) → roh.

## Phase 2 — Review-Härtung
- **Cache-Schlüssel über finales SSML**: in `azure_tts.rs` sicherstellen, dass der Cache über den
  **SSML-Inhalt inkl. `<lang>`-Tags** hasht (nicht über den Anzeigetext) — sonst Kollision
  „Anna Nguyen" de vs. vi.
- **Manuelle Sprach-Korrektur je Name:** optionales `lang`-Feld in der Nutzer-Aussprache-Tabelle,
  um Fehlklassifikationen im Einzelfall zu überschreiben.
- **Fallback-Konsistenz:** optional die Ansage-Quelle (Azure/Web-Speech) **pro Session** festhalten,
  damit bei Netz-Aussetzern nicht „halb nativ / halb deutsch" entsteht.
- **Datenschutz (dokumentieren):** `enabled=false` Default, EU-Region, nur Namen.

## Bewusst NICHT (Scope-Reduktion ggü. IPA-Weg)
Kein Server-Cron „Gap-Report", kein Pro-Turnier-IPA-Lookup-Endpoint, kein Massen-Import der 6
IPA-Lexika — für `<lang>` unnötig. Die Lexika dienen nur als **Klassifikations-Listen**.

## Betroffene Dateien (Schwerpunkt bts-light)
- `src/io/transliterate.ts` — `detectNameLang` erweitern + Konfidenz.
- `src/io/nameLangBase.ts` (neu) — Name→Sprache, generiert aus `db/seed/*.xml`.
- `src/io/announcer.ts` — `nameSsml`/`langWrapSsml` auf erweiterte Locales + Konfidenz.
- `src-tauri/src/azure_tts.rs` — Cache-Key = SSML-Hash.
- `src/types.ts` / `src-tauri/src/config.rs` — optionales Pro-Name-`lang`-Override (Phase 2).
- Doku: `docs/tts-azure-concept.md`, `docs/announcements.md`.

## Verification
- **Spike**: Hörproben (Azure-Key, extern) — Go/No-Go + Stimmen-/Sprach-Verdikt.
- Client: `cargo fmt/clippy/test`, `npm run build`; manuell mit Azure-Stimme + Turnier mit gemischten
  Namen → native Aussprache; niedrige Konfidenz bleibt deutsch; offline → Web-Speech-Fallback.
- Auslieferung wie üblich (Version-Bump, Changelog, PR→Build→Merge→Tag, Auto-Update).

## Prerequisite
- **v0.9.138 mergen + taggen** (IPA-`<phoneme>`-Tier + vorhandener `<lang>`-Fallback).
