//! Digitaler Tablet-Spielzettel: geteilter Zustand, der eingebettete
//! HTTP+WebSocket-Server (LAN-Modus) und der Relay-Client (Cloud-Modus).

pub mod assets;
pub mod mdns;
pub mod monitor;
pub mod relay_client;
pub mod server;
pub mod state;
pub mod winners;
