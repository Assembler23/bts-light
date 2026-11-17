//! Offline-Stimme für Ansagen ohne Internet (Spec `ansage-offline-stimme`,
//! ADR 0104). Bisher nur der Weg vom Text zu den Lauten (V1); die Engine
//! selbst und der Download folgen mit Etappe E2. Die Messung E0 benutzt ihn
//! über `tests/offline_stimme_probe.rs`.

pub mod laute;
