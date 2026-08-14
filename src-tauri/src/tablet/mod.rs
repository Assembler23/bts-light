//! Digitaler Tablet-Spielzettel: geteilter Zustand, der eingebettete
//! HTTP+WebSocket-Server (LAN-Modus) und der Relay-Client (Cloud-Modus).

pub mod assets;
pub mod assign;
pub mod club_logos;
pub mod exclusion;
pub mod mdns;
pub mod monitor;
pub mod officials;
pub mod queue_order;
pub mod relay_client;
pub mod server;
pub mod slave_bridge;
pub mod state;
pub mod timeline;
pub mod tl;
pub mod winners;
