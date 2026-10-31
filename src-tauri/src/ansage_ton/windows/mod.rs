//! Windows-Teil des Ansage-Tons: WASAPI-Ausgabe (cpal) und Windows-Stimme
//! (WinRT/OneCore). Hardware- und Betriebssystem-nah, deshalb ohne
//! CI-Tests — geprüft über die Sonde `tests/ansage_ton_probe.rs`.

pub mod ausgabe;
pub mod winrt_stimme;
