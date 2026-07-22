// Wählt die Azure-Stimme für eine Ansage anhand der Disziplin.
//
// Optionales Feature: In `AzureTtsConfig.discipline_voices` kann je Disziplin
// eine eigene Stimme hinterlegt sein (z. B. Herren-Disziplinen männlich,
// Damen-Disziplinen weiblich — frei wählbar, kein Zwang). Ist für die Disziplin
// nichts (oder Leeres) hinterlegt, gilt die Standard-/Hauptstimme `baseVoice`.
//
// Rein und node-testbar — die Auswahl-Logik soll nicht still kaputtgehen.
export function voiceForDiscipline(baseVoice, disciplineVoices, discipline) {
  if (disciplineVoices && discipline) {
    const v = disciplineVoices[discipline];
    if (typeof v === "string" && v.trim() !== "") {
      return v;
    }
  }
  return baseVoice;
}
