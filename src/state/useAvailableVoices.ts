import { useEffect, useState } from "react";
import { ansageStimmen, type StimmeInfo } from "../api";

// Browser-Stimmen aus speechSynthesis.getVoices(). Die Liste ist initial oft
// leer und wird asynchron befüllt — Browser feuern dann das `voiceschanged`-
// Event. Wir abonnieren es und liefern die jeweils aktuelle Liste.
// Portiert aus badhub-tournament (src/state/useAvailableVoices.ts).
export function useAvailableVoices(): SpeechSynthesisVoice[] {
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>(() => {
    if (
      typeof window === "undefined" ||
      typeof window.speechSynthesis === "undefined"
    ) {
      return [];
    }
    return window.speechSynthesis.getVoices();
  });

  useEffect(() => {
    if (
      typeof window === "undefined" ||
      typeof window.speechSynthesis === "undefined"
    ) {
      return;
    }
    function update() {
      setVoices(window.speechSynthesis.getVoices());
    }
    // Initial-Pull falls die Liste seit dem useState-Initializer befüllt wurde.
    update();
    window.speechSynthesis.addEventListener("voiceschanged", update);
    return () => {
      window.speechSynthesis.removeEventListener("voiceschanged", update);
    };
  }, []);

  return voices;
}

// Filtert nach Sprach-Code (z. B. 'de' liefert 'de-DE', 'de-AT' usw.). Behält
// die Reihenfolge, in der der Browser die Stimmen liefert.
export function voicesForLang(
  voices: SpeechSynthesisVoice[],
  lang: "de" | "en",
): SpeechSynthesisVoice[] {
  return voices.filter((v) =>
    v.lang.toLowerCase().startsWith(lang.toLowerCase()),
  );
}

// ─── Stimmenliste aus Windows (Spec ansage-ausgabegeraet, ADR 0066) ──────
// Seit die Windows-Stimme im Kern spricht, kommt die Auswahl aus Windows
// (OneCore). Unter Linux oder bei einem Kern-Fehler bleibt es bei der
// Browser-Liste. Gespeichert wird der Windows-Name („Microsoft Katja");
// alte Werte aus der Browser-Liste („Microsoft Katja - German (Germany)")
// gelten weiter — der Kern ordnet sie über den Teil vor „ - " zu.

export interface StimmenOption {
  value: string;
  label: string;
}

export function useStimmenOptionen(lang: "de" | "en"): StimmenOption[] {
  const browser = useAvailableVoices();
  const [windows, setWindows] = useState<StimmeInfo[] | null>(null);

  useEffect(() => {
    let aktiv = true;
    ansageStimmen()
      .then((liste) => {
        if (aktiv) setWindows(liste.length > 0 ? liste : null);
      })
      .catch(() => {
        if (aktiv) setWindows(null);
      });
    return () => {
      aktiv = false;
    };
  }, []);

  if (windows) {
    return windows
      .filter((s) => s.sprache.toLowerCase().startsWith(lang))
      .map((s) => ({ value: s.name, label: s.name }));
  }
  return voicesForLang(browser, lang).map((v) => ({
    value: v.voiceURI,
    label: v.name,
  }));
}

/** Wert für das Auswahlfeld: Ein alter Browser-Wert zeigt auf den
 *  passenden Windows-Namen, statt still als „Standardstimme" zu erscheinen. */
export function stimmenAuswahlWert(
  gespeichert: string,
  optionen: StimmenOption[],
): string {
  if (!gespeichert || optionen.some((o) => o.value === gespeichert)) {
    return gespeichert;
  }
  const kurz = gespeichert.split(" - ")[0];
  return optionen.some((o) => o.value === kurz) ? kurz : gespeichert;
}
