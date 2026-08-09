//! Check-In-Stand aus badhub lesen und Eingriffe der Turnierleitung senden.
//!
//! Die Gegenstelle zu `lib/checkin_tl_lib.php` in badhub (Schnitt C). Anders
//! als der Meldelisten-Push in [`super::push`] ist das hier ein **Lesepfad im
//! Poll-Takt**, dazu drei Schreibwege für die Turnierleitung.
//!
//! ## Warum nichts hier je einen Fehler nach oben gibt
//!
//! Der Check-In ist **additiv**: fällt er aus, läuft das Turnier unverändert
//! weiter. Ein `Err` würde im Frontend als roter Kasten landen und die
//! Turnierleitung mitten im Betrieb beunruhigen, obwohl nichts kaputt ist.
//! Stattdessen trägt jede Antwort ihre [`Availability`] mit sich:
//!
//! - [`Availability::Ready`] — badhub hat geantwortet.
//! - [`Availability::Offline`] — keine Verbindung (AK-C3).
//! - [`Availability::Unsupported`] — badhub kennt den Kanal noch nicht,
//!   HTTP 404/400 (AK-C4). bts-light kommt per Auto-Update auf alle
//!   Installationen, badhub wird unabhängig deployt.
//! - [`Availability::Rejected`] — Passwort oder Turnier-GUID passen nicht
//!   (401/403). Der einzige Fall, den die Turnierleitung selbst beheben kann,
//!   und deshalb der einzige, der eine Meldung verdient.
//!
//! C3 und C4 laufen damit durch **denselben** Codepfad wie in der
//! Spezifikation vorgesehen — sie sind gemeinsam getestet.
//!
//! ## Kein lokaler Zwischenspeicher
//!
//! Anders als beim Aussprache-Wörterbuch gibt es hier **keinen Datei-Cache**.
//! AK-C13 verlangt genau das: badhub speichert, bts-light schreibt durch. Ein
//! Cache wäre die zweite Wahrheit, die dieses Modell vermeidet — und ein
//! angezeigter Check-In-Stand von gestern wäre schlimmer als gar keiner.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Kürzer als der 15-Sekunden-Timeout des Pushes: die Sicht wird gepollt, und
/// eine hängende Anfrage soll den nächsten Zyklus nicht überholen.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Ist der Check-In gerade benutzbar — und wenn nicht, warum?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// badhub hat geantwortet.
    Ready,
    /// Keine Verbindung — im reinen LAN-Betrieb der Normalfall (AK-C3).
    Offline,
    /// badhub kennt den Check-In-Kanal noch nicht (404/400, AK-C4).
    Unsupported,
    /// Passwort oder Turnier-GUID passen nicht (401/403).
    Rejected,
}

/// Ein Spieler in einer Klasse, so wie die Turnierleitung ihn sieht.
///
/// Enthält bewusst mehr als die öffentliche Seite: `state` kann `query`
/// („bitte zur Turnierleitung") sein, und `locked` zeigt eine Sperre. Beides
/// verlässt badhub nach außen nie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckinPlayer {
    pub player_id: i64,
    #[serde(default)]
    pub entry_id: i64,
    #[serde(default)]
    pub first: String,
    #[serde(default)]
    pub last: String,
    #[serde(default)]
    pub club: Option<String>,
    #[serde(default)]
    pub nationality: Option<String>,
    /// `open` · `checked_in` · `query`
    #[serde(default = "state_open")]
    pub state: String,
    /// `self` · `partner` · `official` — wodurch der Check-In zustande kam.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub checked_in_at: Option<String>,
}

fn state_open() -> String {
    "open".to_string()
}

impl CheckinPlayer {
    /// Fehlt dieser Spieler noch? Grundlage der Fehlt-Ansage (AK-C7).
    ///
    /// `query` zählt als fehlend: die betreffende Person soll zur
    /// Turnierleitung kommen, ist also gerade nicht abgehakt.
    pub fn is_missing(&self) -> bool {
        self.state != "checked_in"
    }

    /// Anzeigename „Vorname Nachname", ohne doppelte Leerzeichen bei
    /// fehlendem Teil.
    pub fn display_name(&self) -> String {
        match (self.first.trim(), self.last.trim()) {
            ("", last) => last.to_string(),
            (first, "") => first.to_string(),
            (first, last) => format!("{first} {last}"),
        }
    }
}

/// Eine Spielklasse mit ihrem Check-In-Fenster und ihren Meldungen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckinClass {
    pub event_id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub discipline: String,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub closes_at: Option<String>,
    /// Wann das Fenster öffnet — von badhub berechnet, nie hier.
    #[serde(default)]
    pub opens_at: Option<String>,
    /// `unscheduled` · `pending` · `open` · `closed` · `live`.
    ///
    /// **Serverseitig in `Europe/Berlin` berechnet** (Spezifikation B2). Die
    /// Uhr des Windows-Rechners, auf dem bts-light läuft, geht hier bewusst
    /// nicht ein: sie ist auf einem Turnier-Laptop genauso oft falsch gestellt
    /// wie auf einem Handy.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub is_live: bool,
    #[serde(default)]
    pub gemeldet: i64,
    #[serde(default)]
    pub eingecheckt: i64,
    #[serde(default)]
    pub players: Vec<CheckinPlayer>,
}

impl CheckinClass {
    /// Die noch fehlenden Spieler, alphabetisch wie von badhub geliefert.
    pub fn missing(&self) -> Vec<&CheckinPlayer> {
        self.players.iter().filter(|p| p.is_missing()).collect()
    }

    /// Anzeigename der Klasse, notfalls aus der EventID.
    fn label(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            format!("Klasse {}", self.event_id)
        } else {
            name.to_string()
        }
    }
}

/// Ansagetext „Noch N Minuten bis Anmeldeschluss …" (AK-C6).
///
/// `None`, wenn es nichts anzusagen gibt: keine gepflegte Anmeldezeit oder der
/// Schluss ist bereits vorbei. Eine Ansage „noch minus drei Minuten" wäre in
/// der Halle nur Verwirrung.
///
/// `now` kommt herein, statt hier gelesen zu werden — sonst wäre die Funktion
/// nicht prüfbar.
pub fn deadline_text(class: &CheckinClass, now: chrono::NaiveDateTime) -> Option<String> {
    // badhub liefert Berlin-Wandzeit ohne Zeitzonen-Anhang (ADR-0005 dort).
    // Verglichen wird deshalb mit der lokalen Uhr dieses Rechners — beide
    // stehen in derselben Halle.
    let closes = class.closes_at.as_deref().or(class.starts_at.as_deref())?;
    let ziel = chrono::NaiveDateTime::parse_from_str(closes.trim(), "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(closes.trim(), "%Y-%m-%dT%H:%M:%S"))
        .ok()?;

    let minuten = (ziel - now).num_minutes();
    if minuten < 1 {
        return None;
    }

    let einheit = if minuten == 1 { "Minute" } else { "Minuten" };
    Some(format!(
        "Noch {minuten} {einheit} bis Anmeldeschluss {}.",
        class.label()
    ))
}

/// Ansagetext der fehlenden Spieler (AK-C7, C8).
///
/// `None`, wenn niemand fehlt — dann wird der Knopf gar nicht erst angeboten.
///
/// Bis `max_names` Namen werden sie genannt, **darüber nur die Anzahl**. Sonst
/// läuft die Ansage kurz nach Fensteröffnung minutenlang, wenn noch fast
/// niemand eingecheckt ist — und niemand in der Halle hört bis zum Ende zu.
pub fn missing_text(class: &CheckinClass, max_names: u32) -> Option<String> {
    let fehlend = class.missing();
    if fehlend.is_empty() {
        return None;
    }

    let klasse = class.label();
    if fehlend.len() as u32 > max_names {
        let wort = if fehlend.len() == 1 {
            "Anmeldung"
        } else {
            "Anmeldungen"
        };
        return Some(format!("In {klasse} fehlen noch {} {wort}.", fehlend.len()));
    }

    let namen: Vec<String> = fehlend.iter().map(|p| p.display_name()).collect();
    if namen.len() == 1 {
        Some(format!("In {klasse} fehlt noch {}.", namen[0]))
    } else {
        Some(format!("In {klasse} fehlen noch: {}.", namen.join(", ")))
    }
}

/// Was die Turnierleitungs-Sicht anzeigt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckinView {
    pub availability: Availability,
    /// Turniername aus badhub; leer, solange nichts gepusht wurde.
    #[serde(default)]
    pub tournament_name: String,
    #[serde(default)]
    pub classes: Vec<CheckinClass>,
    /// Klartext für die Oberfläche, wenn etwas zu sagen ist.
    #[serde(default)]
    pub message: String,
}

impl CheckinView {
    /// Der Zustand „gar nichts abrufbar" mit Begründung.
    pub fn unavailable(availability: Availability, message: impl Into<String>) -> Self {
        Self {
            availability,
            tournament_name: String::new(),
            classes: Vec::new(),
            message: message.into(),
        }
    }
}

/// Antwortform von `GET /checkin/<GUID>/tl/stand`.
#[derive(Debug, Deserialize)]
struct StateResponse {
    #[serde(default)]
    tournament: Option<TournamentInfo>,
    #[serde(default)]
    classes: Vec<CheckinClass>,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct TournamentInfo {
    #[serde(default)]
    name: String,
}

/// Antwortform der beiden Schreibwege.
#[derive(Debug, Deserialize)]
struct WriteResponse {
    #[serde(default)]
    message: String,
}

/// HTTP-Client für den Check-In-Kanal.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("HTTP-Client-Erzeugung kann nicht fehlschlagen")
}

/// Adresse eines TL-Endpunkts unter der badhub-Basis.
fn tl_url(base: &str, uuid: &str, pfad: &str) -> String {
    format!(
        "{}/checkin/{}/tl/{}",
        base.trim_end_matches('/'),
        uuid,
        pfad
    )
}

/// Den vollständigen Check-In-Stand abrufen (AK-C1, C5).
///
/// **Liefert nie `Err`.** Warum, steht im Modulkopf.
pub async fn fetch_state(
    client: &reqwest::Client,
    base: &str,
    password: &str,
    uuid: &str,
) -> CheckinView {
    let response = match client
        .get(tl_url(base, uuid, "stand"))
        .bearer_auth(password)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            return CheckinView::unavailable(
                Availability::Offline,
                "Der Check-In braucht Internet — badhub ist gerade nicht erreichbar.",
            )
        }
    };

    match response.status().as_u16() {
        200 => {}
        400 | 404 => {
            return CheckinView::unavailable(
                Availability::Unsupported,
                "Dieses badhub kennt den Check-In noch nicht.",
            )
        }
        401 | 403 => {
            return CheckinView::unavailable(
                Availability::Rejected,
                "badhub hat den Zugang abgelehnt — Liveticker-Passwort und Turnier-Kennung prüfen.",
            )
        }
        other => {
            // 5xx und alles Übrige: wie offline behandeln. Der nächste Poll
            // versucht es erneut, ohne dass jemand etwas tun müsste.
            return CheckinView::unavailable(
                Availability::Offline,
                format!("badhub antwortete mit HTTP {other}."),
            );
        }
    }

    let body: StateResponse = match response.json().await {
        Ok(b) => b,
        Err(_) => {
            // Gültiger Status, unlesbarer Rumpf — praktisch immer eine
            // Fehlerseite von einem Proxy davor.
            return CheckinView::unavailable(
                Availability::Offline,
                "badhub hat unverständlich geantwortet.",
            );
        }
    };

    CheckinView {
        availability: Availability::Ready,
        tournament_name: body.tournament.map(|t| t.name).unwrap_or_default(),
        classes: body.classes,
        message: body.message,
    }
}

/// Ergebnis eines Schreibversuchs — das Frontend zeigt bei `Err` den Text an.
pub type WriteResult = Result<(), String>;

/// Gemeinsame Auswertung beider Schreibwege.
///
/// Anders als beim Lesen wird ein Fehlschlag hier **gemeldet**: Die
/// Turnierleitung hat gerade geklickt und muss wissen, dass nichts passiert
/// ist. Eine stille Ablehnung wäre die schlimmere Variante — sie sähe aus wie
/// Erfolg, und der nächste Poll holte den alten Stand zurück (AK-C14).
async fn auswerten(antwort: Result<reqwest::Response, reqwest::Error>) -> WriteResult {
    let response = match antwort {
        Ok(r) => r,
        Err(_) => {
            return Err(
                "Keine Verbindung zu badhub — die Änderung wurde nicht gespeichert.".to_string(),
            )
        }
    };

    let status = response.status().as_u16();
    // Den Text von badhub bevorzugen: er ist auf die Turnierleitung gemünzt
    // („Der Anmeldeschluss liegt vor der Anfangszeit") und weiß mehr als ein
    // Statuscode hier je wissen könnte.
    let text = response
        .json::<WriteResponse>()
        .await
        .map(|b| b.message)
        .unwrap_or_default();

    match status {
        200 => Ok(()),
        400 | 404 if text.is_empty() => {
            Err("Dieses badhub kennt den Check-In noch nicht.".to_string())
        }
        401 | 403 if text.is_empty() => Err("badhub hat den Zugang abgelehnt.".to_string()),
        _ if !text.is_empty() => Err(text),
        other => Err(format!("badhub antwortete mit HTTP {other}.")),
    }
}

/// Einen Spieler setzen, zurücksetzen oder entsperren (AK-C2).
///
/// `action` ist `check_in`, `reset` oder `unlock`. Zurücksetzen sperrt den
/// Selbst-Check-In — das entscheidet badhub, nicht diese Funktion.
pub async fn set_player(
    client: &reqwest::Client,
    base: &str,
    password: &str,
    uuid: &str,
    event_id: i64,
    player_id: i64,
    action: &str,
) -> WriteResult {
    let body = serde_json::json!({
        "event_id": event_id,
        "player_id": player_id,
        "action": action,
    });

    auswerten(
        client
            .post(tl_url(base, uuid, "spieler"))
            .bearer_auth(password)
            .json(&body)
            .send()
            .await,
    )
    .await
}

/// Anfangszeit und Anmeldeschluss einer Klasse setzen (AK-C12).
///
/// `None` löscht den jeweiligen Wert. Die Plausibilität prüft badhub
/// (AK-C15) — eine zweite Regel hier wäre eine zweite Wahrheit, die
/// auseinanderlaufen kann.
pub async fn set_times(
    client: &reqwest::Client,
    base: &str,
    password: &str,
    uuid: &str,
    event_id: i64,
    starts_at: Option<&str>,
    closes_at: Option<&str>,
) -> WriteResult {
    let body = serde_json::json!({
        "event_id": event_id,
        "starts_at": starts_at,
        "closes_at": closes_at,
    });

    auswerten(
        client
            .post(tl_url(base, uuid, "klasse"))
            .bearer_auth(password)
            .json(&body)
            .send()
            .await,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Mini-HTTP-Mock nach dem Muster aus `push.rs`: eine Anfrage, eine
    /// vorgegebene Antwort. Liefert die Basis-Adresse zurück.
    async fn spawn_mock(status_line: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = sock.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(response.as_bytes()).await;
        });
        format!("http://{addr}")
    }

    const UUID: &str = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA";

    #[test]
    fn url_wird_unter_der_basis_gebaut() {
        assert_eq!(
            tl_url("https://badhub.de", UUID, "stand"),
            format!("https://badhub.de/checkin/{UUID}/tl/stand")
        );
        // Ein abschließender Schrägstrich in der Config darf keine doppelte
        // Trennung erzeugen — die URL wäre sonst 404.
        assert_eq!(
            tl_url("https://badhub.de/", UUID, "stand"),
            format!("https://badhub.de/checkin/{UUID}/tl/stand")
        );
    }

    #[tokio::test]
    async fn stand_wird_gelesen() {
        let body = r#"{"ok":true,"tournament":{"uuid":"x","name":"Köpi-Cup"},
            "classes":[{"event_id":5,"name":"MX A","discipline":"mixed",
            "starts_at":"2026-08-15 09:00:00","opens_at":"2026-08-15 08:00:00",
            "state":"open","gemeldet":2,"eingecheckt":1,
            "players":[
              {"player_id":1,"first":"Anna","last":"Alt","state":"checked_in","source":"self"},
              {"player_id":2,"first":"Ben","last":"Bach","state":"open","locked":true}
            ]}]}"#;
        let base = spawn_mock("200 OK", body).await;
        let view = fetch_state(&build_client(), &base, "pw", UUID).await;

        assert_eq!(view.availability, Availability::Ready);
        assert_eq!(view.tournament_name, "Köpi-Cup");
        assert_eq!(view.classes.len(), 1);
        let klasse = &view.classes[0];
        assert_eq!(klasse.eingecheckt, 1);
        assert_eq!(klasse.players.len(), 2);
        // Sperre und Herkunft kommen an — sie sind der Grund, warum es diesen
        // Kanal neben dem öffentlichen überhaupt gibt.
        assert!(klasse.players[1].locked);
        assert_eq!(klasse.players[0].source.as_deref(), Some("self"));
    }

    #[tokio::test]
    async fn fehlende_spieler_werden_erkannt() {
        let body = r#"{"ok":true,"tournament":{"name":"T"},"classes":[{"event_id":1,
            "players":[
              {"player_id":1,"first":"Anna","last":"Alt","state":"checked_in"},
              {"player_id":2,"first":"Ben","last":"Bach","state":"open"},
              {"player_id":3,"first":"Cem","last":"Cetin","state":"query"}
            ]}]}"#;
        let base = spawn_mock("200 OK", body).await;
        let view = fetch_state(&build_client(), &base, "pw", UUID).await;

        let fehlend = view.classes[0].missing();
        // `query` zählt als fehlend: die Person soll zur Turnierleitung, ist
        // also gerade nicht abgehakt.
        assert_eq!(fehlend.len(), 2);
        assert_eq!(fehlend[0].display_name(), "Ben Bach");
        assert_eq!(fehlend[1].display_name(), "Cem Cetin");
    }

    #[tokio::test]
    async fn unbekannter_endpunkt_heisst_nicht_verfuegbar() {
        // AK-C4: altes badhub, das den Kanal nicht kennt.
        let base = spawn_mock("404 Not Found", "not found").await;
        let view = fetch_state(&build_client(), &base, "pw", UUID).await;

        assert_eq!(view.availability, Availability::Unsupported);
        assert!(view.classes.is_empty());
    }

    #[tokio::test]
    async fn abgelehnter_zugang_wird_unterschieden() {
        // Der einzige Fall, den die Turnierleitung selbst beheben kann —
        // deshalb darf er nicht als „offline" verschwinden.
        let base = spawn_mock("403 Forbidden", r#"{"ok":false}"#).await;
        let view = fetch_state(&build_client(), &base, "pw", UUID).await;

        assert_eq!(view.availability, Availability::Rejected);
        assert!(!view.message.is_empty());
    }

    #[tokio::test]
    async fn serverfehler_verhaelt_sich_wie_offline() {
        let base = spawn_mock("500 Internal Server Error", "boom").await;
        let view = fetch_state(&build_client(), &base, "pw", UUID).await;

        assert_eq!(view.availability, Availability::Offline);
    }

    #[tokio::test]
    async fn keine_verbindung_ergibt_offline_statt_fehler() {
        // AK-C3: kein Internet. Nichts darf hier ein Err werden — das
        // Programm bleibt vollständig bedienbar.
        let view = fetch_state(
            &build_client(),
            "http://127.0.0.1:1", // dort horcht niemand
            "pw",
            UUID,
        )
        .await;

        assert_eq!(view.availability, Availability::Offline);
        assert!(view.classes.is_empty());
        assert!(!view.message.is_empty());
    }

    #[tokio::test]
    async fn turnier_ohne_push_ist_leer_aber_verfuegbar() {
        // badhub antwortet mit 200 und leerem Stand, solange noch nichts
        // gepusht wurde. Das ist NICHT „nicht verfügbar" — sonst sähe der
        // Turniertag-Morgen aus wie ein veraltetes badhub.
        let body = r#"{"ok":true,"tournament":null,"classes":[],
            "message":"Fuer dieses Turnier liegt noch keine Meldeliste vor."}"#;
        let base = spawn_mock("200 OK", body).await;
        let view = fetch_state(&build_client(), &base, "pw", UUID).await;

        assert_eq!(view.availability, Availability::Ready);
        assert!(view.classes.is_empty());
        assert!(!view.message.is_empty());
    }

    #[tokio::test]
    async fn eingriff_wird_bestaetigt() {
        let base = spawn_mock("200 OK", r#"{"ok":true}"#).await;
        let r = set_player(&build_client(), &base, "pw", UUID, 5, 101, "check_in").await;

        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn abgelehnter_eingriff_meldet_den_text_von_badhub() {
        // Die Begründung stammt von badhub und ist auf die Turnierleitung
        // gemünzt — sie hier zu ersetzen würde Auskunft vernichten.
        let base = spawn_mock(
            "400 Bad Request",
            r#"{"ok":false,"message":"Der Anmeldeschluss liegt vor der Anfangszeit."}"#,
        )
        .await;
        let r = set_times(
            &build_client(),
            &base,
            "pw",
            UUID,
            5,
            Some("2026-08-15 09:00:00"),
            Some("2026-08-15 08:00:00"),
        )
        .await;

        assert_eq!(
            r.unwrap_err(),
            "Der Anmeldeschluss liegt vor der Anfangszeit."
        );
    }

    #[tokio::test]
    async fn schreiben_ohne_verbindung_wird_gemeldet_nicht_verschluckt() {
        // AK-C14: ohne Verbindung sind die Zeiten nur lesbar, und ein
        // Änderungsversuch wird verständlich abgelehnt. Stillschweigen wäre
        // hier schlimmer als der Fehler — es sähe aus wie Erfolg.
        let r = set_times(
            &build_client(),
            "http://127.0.0.1:1",
            "pw",
            UUID,
            5,
            Some("2026-08-15 09:00:00"),
            None,
        )
        .await;

        assert!(r.is_err());
        assert!(r.unwrap_err().contains("badhub"));
    }

    /// Klasse mit `anzahl` Meldungen, davon `da` eingecheckt.
    fn klasse_mit(anzahl: usize, da: usize) -> CheckinClass {
        let players = (0..anzahl)
            .map(|i| CheckinPlayer {
                player_id: i as i64 + 1,
                entry_id: 0,
                first: format!("Vor{i}"),
                last: format!("Nach{i}"),
                club: None,
                nationality: None,
                state: if i < da { "checked_in" } else { "open" }.to_string(),
                source: None,
                locked: false,
                checked_in_at: None,
            })
            .collect();
        CheckinClass {
            event_id: 5,
            name: "Herrendoppel B".into(),
            discipline: "mens_doubles".into(),
            starts_at: Some("2026-08-15 09:00:00".into()),
            closes_at: Some("2026-08-15 09:30:00".into()),
            opens_at: None,
            state: "open".into(),
            is_live: false,
            gemeldet: anzahl as i64,
            eingecheckt: da as i64,
            players,
        }
    }

    fn zeitpunkt(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn ansage_nennt_die_verbleibenden_minuten() {
        let k = klasse_mit(4, 1);
        assert_eq!(
            deadline_text(&k, zeitpunkt("2026-08-15 09:05:00")).unwrap(),
            "Noch 25 Minuten bis Anmeldeschluss Herrendoppel B."
        );
        // Einzahl — „Noch 1 Minuten" wäre in einer gesprochenen Ansage
        // besonders auffällig.
        assert_eq!(
            deadline_text(&k, zeitpunkt("2026-08-15 09:29:00")).unwrap(),
            "Noch 1 Minute bis Anmeldeschluss Herrendoppel B."
        );
    }

    #[test]
    fn ansage_entfaellt_wenn_der_schluss_vorbei_ist() {
        let k = klasse_mit(4, 1);
        // „Noch minus drei Minuten" wäre in der Halle nur Verwirrung.
        assert!(deadline_text(&k, zeitpunkt("2026-08-15 09:31:00")).is_none());
        // Und exakt zum Anmeldeschluss ebenfalls nichts mehr.
        assert!(deadline_text(&k, zeitpunkt("2026-08-15 09:30:00")).is_none());
    }

    #[test]
    fn ohne_anmeldeschluss_gilt_die_anfangszeit() {
        // Fehlt der eigene Anmeldeschluss, ist er laut Spezifikation gleich
        // der Anfangszeit — die Ansage darf deshalb nicht ausfallen.
        let mut k = klasse_mit(4, 1);
        k.closes_at = None;
        assert_eq!(
            deadline_text(&k, zeitpunkt("2026-08-15 08:50:00")).unwrap(),
            "Noch 10 Minuten bis Anmeldeschluss Herrendoppel B."
        );
    }

    #[test]
    fn ohne_jede_zeit_gibt_es_keine_ansage() {
        let mut k = klasse_mit(4, 1);
        k.closes_at = None;
        k.starts_at = None;
        assert!(deadline_text(&k, zeitpunkt("2026-08-15 08:50:00")).is_none());
    }

    #[test]
    fn fehlt_niemand_gibt_es_keine_ansage() {
        // AK-C8.
        let k = klasse_mit(3, 3);
        assert!(missing_text(&k, 8).is_none());
    }

    #[test]
    fn bis_zur_grenze_werden_namen_genannt() {
        // AK-C7, unterhalb der Grenze.
        let k = klasse_mit(3, 1);
        assert_eq!(
            missing_text(&k, 8).unwrap(),
            "In Herrendoppel B fehlen noch: Vor1 Nach1, Vor2 Nach2."
        );
    }

    #[test]
    fn ein_einzelner_fehlender_bekommt_die_einzahl() {
        let k = klasse_mit(2, 1);
        assert_eq!(
            missing_text(&k, 8).unwrap(),
            "In Herrendoppel B fehlt noch Vor1 Nach1."
        );
    }

    #[test]
    fn genau_an_der_grenze_werden_die_namen_noch_genannt() {
        // Die Grenze gehört zum Namens-Fall: „bis zu N Namen" heißt
        // einschließlich N.
        let k = klasse_mit(4, 1);
        let text = missing_text(&k, 3).unwrap();
        assert!(text.contains("Vor1 Nach1"), "erwartet Namen, war: {text}");
    }

    #[test]
    fn ueber_der_grenze_kommt_nur_die_anzahl() {
        // AK-C7: sonst läuft die Ansage kurz nach Fensteröffnung minutenlang.
        let k = klasse_mit(24, 1);
        assert_eq!(
            missing_text(&k, 8).unwrap(),
            "In Herrendoppel B fehlen noch 23 Anmeldungen."
        );
    }

    #[test]
    fn rueckfrage_zaehlt_als_fehlend() {
        // Wer eine Rückfrage hat, soll zur Turnierleitung — er ist gerade
        // nicht abgehakt und gehört deshalb in die Ansage.
        let mut k = klasse_mit(2, 2);
        k.players[1].state = "query".into();
        assert_eq!(
            missing_text(&k, 8).unwrap(),
            "In Herrendoppel B fehlt noch Vor1 Nach1."
        );
    }

    #[test]
    fn klasse_ohne_namen_bekommt_eine_kennung() {
        let mut k = klasse_mit(2, 1);
        k.name = String::new();
        assert!(missing_text(&k, 8).unwrap().starts_with("In Klasse 5 "));
    }

    #[test]
    fn anzeigename_kommt_auch_mit_fehlenden_teilen_zurecht() {
        let mut p = CheckinPlayer {
            player_id: 1,
            entry_id: 0,
            first: "Anna".into(),
            last: "Alt".into(),
            club: None,
            nationality: None,
            state: "open".into(),
            source: None,
            locked: false,
            checked_in_at: None,
        };
        assert_eq!(p.display_name(), "Anna Alt");
        p.first = String::new();
        assert_eq!(p.display_name(), "Alt");
        p.last = String::new();
        assert_eq!(p.display_name(), "");
    }
}
