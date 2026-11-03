# Spec: Weitere Sprachdienste für Ansagen (ElevenLabs, OpenAI, Gemini)

Stand: 02.11.2026 · Status: **freigegeben 02.11.2026**

## Ziel

Die Turnierleitung soll beim Turnier hören können, ob ElevenLabs schwierige
Spielernamen besser ausspricht als Azure, und dafür **mit einem Klick**
zwischen den Diensten wechseln. Der Schreibtisch-Hörvergleich (30 Namen,
02.11.2026) war nicht eindeutig genug; entscheiden soll der echte Hallenbetrieb.

## Entscheidungen (mit dem Nutzer geklärt, 02.11.2026)

| # | Frage | Entscheidung |
|---|---|---|
| E1 | Wie umschalten? | Auswahl **„Sprachdienst"** auf der Seite **Ansagen**: Azure · ElevenLabs v2 · ElevenLabs v3 · ElevenLabs v4 · ElevenLabs Flash v2.5 (v4 und Flash auf Wunsch ergänzt, Modelle `eleven_v4` und `eleven_flash_v2_5`; Flash wegen des etwa halben Zeichenpreises). Wirkt ab der nächsten Ansage, ohne Neustart. |
| E2 | Welche Namen bekommt ElevenLabs? | Den Namen **roh** wie aus BTP. Es gelten nur die **eigenen** Korrekturen aus der Nutzer-Tabelle (Ersatzschreibweise). Die geteilten Community-Korrekturen, das Basis-Wörterbuch und die Lautschrift (IPA) entfallen: Sie sind für deutsche Stimmen gemacht und würden die mehrsprachigen ElevenLabs-Stimmen eher verschlechtern. |
| E3 | Wohin mit dem Schlüssel? | Feld in den Ansage-Einstellungen, **nicht** im Export (`identity_bundle` leert ihn wie den Azure-Schlüssel), **nicht** eingebettet. Ansage-Slaves erben ihn vom Master auf demselben Weg wie heute Azure (ADR 0003). |

## Verhalten

- **Standard bleibt Azure.** Auf ElevenLabs wird nur gewechselt, wenn jemand es auswählt.
- **Ausweichweg** bei fehlendem Schlüssel oder einem Fehler (Netz, Kontingent leer, 401): Die Ansage fällt wie heute auf die Windows-Stimme zurück, und der vorhandene Hinweisbanner nennt dazu den Grund. Ein stiller Wechsel auf Azure findet **nicht** statt, sonst würde der Hörtest unbemerkt verfälscht.
- **Tempo**: Der Regler wirkt auch hier und wird auf `voice_settings.speed` abgebildet. ElevenLabs erlaubt 0,7 bis 1,2, Werte außerhalb werden auf diese Grenzen gesetzt.
- **Pausen**: 350 ms zwischen den Blöcken wie bei Azure. Bei v2 als `<break time="0.35s" />`. Bei v3, v4 und Flash als Satzpunkt, weil Break-Tags nur für v2 dokumentiert sind; ob v3/v4 den Tag mitsprächen, ist ungeprüft, deshalb lieber gar nicht senden.
- **Stimme**: Auswahl aus den ElevenLabs-Standardstimmen. Voreingestellt ist „Sarah" (`EXAVITQu4vr4xnSDxMaL`), alternativ kann eine Stimmen-ID eingetippt werden. Es gibt **eine** Stimme für alle Disziplinen; die Stimme je Disziplin gilt nur für Azure.
- **Cache**: Wie bei Azure in `tts-cache/`, der Schlüssel ist ein Hash über Dienst, Modell, Stimme, Tempo und Text. Das spart Kontingent bei Wiederholungen. Die Ansage-Seiten im Browser vertonen wie bisher **ohne** Platten-Cache.

## Umsetzung (eine Naht statt neuem Pfad)

Alle Aufrufer bleiben, wie sie sind. Das Frontend baut weiter SSML, und der Kern entscheidet, wohin es geht.

1. **`config.rs`**: Die Felder hängen an der vorhandenen `AzureTtsConfig` (`azure_tts.dienst` = `azure` | `elevenlabs_v2` | `elevenlabs_v3` | `elevenlabs_v4` | `elevenlabs_flash`, Standard `azure`; dazu `elevenlabs_key` und `elevenlabs_voice`). Grund: Diese Struktur läuft schon durch alle Ansage-Wege und die Vererbung, sodass kein Aufrufer neu verdrahtet werden muss. `dienst` ist Text statt Enum, damit ein unbekannter Wert die Config nicht unlesbar macht; er gilt dann als Azure. Der Schalter `enabled` („Cloud-Stimme verwenden") gilt für beide Dienste. Den ElevenLabs-Schlüssel leeren `identity_bundle` bzw. `apply_imported_identity` genau wie den Azure-Schlüssel.
2. **`src-tauri/src/elevenlabs_tts.rs`** (neu, reine Funktionen getestet):
   - `ssml_zu_text(ssml, modell) -> (text, speed)`: Tags entfernen, Entities zurückwandeln, `<break>` in die Modell-Form bringen, `<prosody rate="-20%">` in speed umrechnen;
   - `synthesize(key, voice, modell, text, speed, cache)` mit mp3, 10 s Timeout und gekürzter Fehlermeldung.
3. **`commands.rs`**: `azure_tts_speak` und `ansage_vertonen` verzweigen nach `tts_dienst`. Der Name `azure_tts_speak` bleibt, damit das Frontend unverändert bleibt.
4. **Vererbung**: `relay-proto` `AzureTtsShare` bekommt optional `dienst` und `elevenlabs { key, voice_id }`, beide `serde(default)`. Ein altes Relay oder ein alter Master schicken nichts davon, dann gilt Azure.
5. **Frontend**:
   - `azureOption` gilt als vollständig, wenn der gewählte Dienst vollständig ist, und trägt `dienst` mit;
   - die `prepare*`-Funktionen nehmen bei ElevenLabs nur die Nutzer-Korrekturen und keine IPA (E2);
   - auf der Seite Ansagen: Auswahl „Sprachdienst", ElevenLabs-Block mit Schlüssel und Stimme, Test-Knopf wie bei Azure.
6. **Doku**: `docs/announcements.md` (Bedienung, Abschnitt ElevenLabs), `docs/einstellungen.md`, `docs/cloud-relay.md` (Share-Felder), Changelog, Roadmap.

## Erweiterung 03.11.2026 (Nutzer-Wünsche während der Umsetzung)

| # | Wunsch / Befund | Umsetzung |
|---|---|---|
| E4 | ElevenLabs **v4** und **Flash v2.5** (günstiger) anbieten | `elevenlabs_v4` → `eleven_v4`, `elevenlabs_flash` → `eleven_flash_v2_5`; beide mit echtem Schlüssel aus dem Kern vertont. Pausen als Satzpunkt. |
| E5 | **OpenAI Audio API** als weitere Option | `openai` → `gpt-4o-mini-tts`, MP3, Stimme z. B. „coral". Kein Tempo-Parameter wirksam → Tempo und Ton als `instructions`. **Ungeprüft gegen den echten Dienst** (kein Schlüssel). |
| E6 | **Gemini API** | Nur **Google AI Studio** mit API-Schlüssel (Nutzer-Entscheidung), **kein Vertex AI** (Dienstkonto + Token-Erneuerung, zu viel für Plug-and-play). `gemini_flash`/`gemini_pro` → `gemini-2.5-flash-preview-tts`/`gemini-2.5-pro-preview-tts`. Gemini liefert PCM → WAV. Tempo/Ton als Anweisung vor dem Text. Zeitlimit 20 s. Gratis-Schlüssel: Google darf Eingaben zur Produktverbesserung nutzen, enge Limits → für den Einsatz Abrechnung aktivieren (Hinweis in UI und Doku). **Ungeprüft gegen den echten Dienst.** |
| R1 | Review: Sofortwechsel auf einen Dienst ohne Schlüssel schaltete die Cloud-Stimme ab — auch für alle fernen Hallen | Sofort gespeichert wird nur, wenn der Zieldienst im gespeicherten Stand vollständig ist; sonst erst mit „Speichern". |
| R2 | Review: Die Share ist über den Namespace lesbar | Sie trägt **nur** den Zugang des gewählten Dienstes; bei Nicht-Azure auch keine Azure-Felder. Alte Slaves bleiben dann still bei ihrer Ersatzstimme. |
| R4 | Review: OpenAI-/Gemini-Schlüssel gelten fürs ganze Konto und sind über den Namespace lesbar wie der Azure-Schlüssel (ADR 0003) | Bewusst wie Azure belassen (Hörtest), aber Doku empfiehlt eingeschränkte Schlüssel mit Limit. Vor einem dauerhaften Einsatz: Share nur an authentifizierte Slaves. |
| R3 | Review: `language_code: de` erzwingt deutsche Aussprache fremder Namen | Wird nicht mehr gesendet. |
| E7 | Erkennen, ob ein Dienst noch funktioniert (z. B. Token/Kontingent alle) | Knopf **„Dienste prüfen"** → Command `tts_dienste_pruefen`: Probe „Test." ohne Cache an jeden Dienst mit gespeichertem Schlüssel, Fehler verständlich eingeordnet (`pruef_fehler_einordnen`). |

Die Umwandlung SSML → Klartext liegt seitdem in `tts_text.rs` und gilt für
alle drei Dienste; die Dienstliste im Frontend (`io/sprachdienst.mjs`) hat
einen Drift-Test gegen `config::dienst_art`.

## Datenschutz

Spielernamen gehen bei Auswahl von ElevenLabs, OpenAI oder Gemini an einen
weiteren Dienst (alle USA). Das ist derselbe Zweck wie bei Azure, die Ansage, und nur
**nach aktiver Auswahl**. Im ElevenLabs-Block steht dazu ein Hinweissatz.

## Nicht im Umfang

Lautschrift für ElevenLabs, eigene Stimmen je Disziplin, Stimmen-Klonen und ein
dauerhafter A/B-Modus je Ansage.

## Offen fürs Turnier

Höreindruck v2/v3/v4 gegen Azure; ob v3/v4 auf den Satzpunkt eine hörbare Pause
macht; Latenz in der Halle (noch nicht gemessen; der Cache hilft nur bei
Wiederholungen).
