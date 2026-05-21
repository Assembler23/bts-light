import { useEffect, useState } from "react";

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
