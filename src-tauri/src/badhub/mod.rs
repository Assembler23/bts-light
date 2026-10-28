//! Badhub-Anbindung: Übersetzung eines `BtpSnapshot` in das Liveticker-
//! Format von badhub.de und HTTP-Push.

pub mod checkin_state;
pub mod diff;
pub mod payload;
pub mod push;
pub mod tp_guid;
pub mod tp_takt;
pub mod tp_upload;
