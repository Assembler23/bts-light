//! Lesende Aufrufe gegen die badhub-API `/api/torg/v1` (badhub-Spec,
//! Abschnitt „API“): Kopplung, Turnierpaket, Ereignisse.
//!
//! Fehlertexte nennen nur Statuscodes, nie Antwortinhalte — die Antworten
//! tragen Personennamen.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use turnierkern::TurnierEreignis;

use super::paket::Paket;

/// Höchstgröße einer Antwort (Paket eines großen Turniers mit allen
/// Ereignissen liegt weit darunter).
const MAX_ANTWORT: usize = 32 * 1024 * 1024;

/// Fehler eines Aufrufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiFehler {
    /// Keine Verbindung, Zeitüberschreitung.
    Netz(String),
    /// 401/403: Token ungültig oder widerrufen.
    Zugang,
    /// Kopplungscode abgelehnt (ungültig, abgelaufen, schon eingelöst).
    CodeAbgelehnt,
    /// Zu viele Versuche (429).
    Drossel,
    /// Anderer Status.
    Status(u16),
    /// Antwort nicht im erwarteten Format.
    Format(String),
}

impl std::fmt::Display for ApiFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiFehler::Netz(e) => write!(f, "badhub nicht erreichbar ({e})"),
            ApiFehler::Zugang => write!(
                f,
                "badhub lehnt den Zugang dieses PCs ab — Kopplung widerrufen? Bitte neu koppeln."
            ),
            ApiFehler::CodeAbgelehnt => write!(
                f,
                "Der Kopplungscode ist ungültig oder abgelaufen. Bitte in badhub einen neuen erzeugen."
            ),
            ApiFehler::Drossel => write!(f, "Zu viele Versuche — bitte kurz warten."),
            ApiFehler::Status(s) => write!(f, "badhub antwortet mit Status {s}"),
            ApiFehler::Format(e) => write!(f, "Antwort von badhub unlesbar ({e})"),
        }
    }
}

#[derive(Serialize)]
struct KoppelnAnfrage<'a> {
    code: &'a str,
    install_id: &'a str,
    name: &'a str,
}

/// Antwort auf `POST /koppeln`.
#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct KopplungAntwort {
    pub token: String,
    pub turnier: KopplungTurnier,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KopplungTurnier {
    pub id: i64,
    pub uuid: String,
    pub name: String,
}

/// Höchstzahl Ereignisse je `GET /turnier/ereignisse` (`max=`). badhub darf
/// weniger liefern — maßgeblich für „fertig“ ist der Kopf `n`.
pub const SEITE: usize = 500;

/// Antwort auf `GET /turnier/ereignisse` — als Liste (B1), als Objekt mit
/// `ereignisse` oder seit dem Paket in Teilen als `{ab, n, ereignisse}` mit
/// dem Kopf `n` (höchstes Ereignis in badhub).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EreignisAntwort {
    Liste(Vec<TurnierEreignis>),
    Objekt {
        ereignisse: Vec<TurnierEreignis>,
        #[serde(default)]
        n: Option<u64>,
    },
}

/// Eine Seite Ereignisse samt Kopf (`None` bei einem badhub ohne Kopf).
#[derive(Debug, Clone, PartialEq)]
pub struct EreignisSeite {
    pub ereignisse: Vec<TurnierEreignis>,
    pub kopf: Option<u64>,
}

/// Kopf eines gekappten Pakets (badhub liefert höchstens 2000 Ereignisse):
/// `ereignisse_bis_n` = letztes enthaltene Ereignis, `n` = Kopf. Bewusst
/// nicht Teil von [`Paket`] — das wird so gespeichert, wie es ist, und
/// gilt nach dem Nachladen als vollständig.
#[derive(Debug, Default, Deserialize)]
struct PaketKopf {
    #[serde(default)]
    n: Option<u64>,
    #[serde(default)]
    ereignisse_bis_n: Option<u64>,
}

impl PaketKopf {
    /// Bis wohin nachzuladen ist; `None` = vollständig. Fehlt
    /// `ereignisse_bis_n` (badhub vor dem Paket in Teilen), gilt das Paket
    /// als vollständig.
    fn nachladen_bis(&self) -> Option<u64> {
        match (self.ereignisse_bis_n, self.n) {
            (Some(bis), Some(kopf)) if bis < kopf => Some(kopf),
            _ => None,
        }
    }
}

/// Ergebnis von `GET /turnier/paket`.
#[derive(Debug, Clone, PartialEq)]
pub enum PaketAntwort {
    /// 304: Stammdaten unverändert.
    Unveraendert,
    Neu {
        paket: Box<Paket>,
        etag: Option<String>,
        /// Gekapptes Paket: Kopf `n`, bis zu dem über
        /// `GET turnier/ereignisse` nachzuladen ist. `None` = vollständig.
        nachladen_bis: Option<u64>,
    },
}

fn client(zeit: Duration) -> Result<reqwest::Client, ApiFehler> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(zeit)
        // Keine Weiterleitungen: ein https→http-Sprung auf demselben Host
        // schickte das Token sonst im Klartext.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiFehler::Netz(e.to_string()))
}

fn status_fehler(status: reqwest::StatusCode) -> ApiFehler {
    match status.as_u16() {
        401 | 403 => ApiFehler::Zugang,
        429 => ApiFehler::Drossel,
        s => ApiFehler::Status(s),
    }
}

async fn koerper(antwort: reqwest::Response) -> Result<Vec<u8>, ApiFehler> {
    let bytes = antwort
        .bytes()
        .await
        .map_err(|e| ApiFehler::Netz(e.to_string()))?;
    if bytes.len() > MAX_ANTWORT {
        return Err(ApiFehler::Format("Antwort zu groß".into()));
    }
    Ok(bytes.to_vec())
}

fn netz(e: reqwest::Error) -> ApiFehler {
    // Ohne URL: die Basis ist bekannt, die Meldung soll kurz bleiben.
    ApiFehler::Netz(e.without_url().to_string())
}

/// Löst einen Kopplungscode ein.
pub async fn koppeln(
    basis: &str,
    code: &str,
    install_id: &str,
    name: &str,
) -> Result<KopplungAntwort, ApiFehler> {
    let body = serde_json::to_vec(&KoppelnAnfrage {
        code,
        install_id,
        name,
    })
    .map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .post(format!("{basis}/koppeln"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            400 | 401 | 403 | 404 | 409 | 410 | 422 => ApiFehler::CodeAbgelehnt,
            _ => status_fehler(status),
        });
    }
    let text = koerper(antwort).await?;
    serde_json::from_slice(&text).map_err(|_| ApiFehler::Format("Kopplung".into()))
}

/// Lädt das Turnierpaket; mit `etag` nur, wenn sich die Stammdaten geändert
/// haben.
pub async fn paket(
    basis: &str,
    token: &str,
    etag: Option<&str>,
) -> Result<PaketAntwort, ApiFehler> {
    let mut anfrage = client(Duration::from_secs(60))?
        .get(format!("{basis}/turnier/paket"))
        .bearer_auth(token)
        .header("Accept", "application/json");
    if let Some(e) = etag {
        anfrage = anfrage.header("If-None-Match", e);
    }
    let antwort = anfrage.send().await.map_err(netz)?;
    let status = antwort.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(PaketAntwort::Unveraendert);
    }
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let header_etag = antwort
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let text = koerper(antwort).await?;
    let paket: Paket = serde_json::from_slice(&text).map_err(|e| {
        // Nur Zeile/Spalte, kein Inhalt (Namen) ins Log.
        ApiFehler::Format(format!(
            "Turnierpaket, Zeile {} Spalte {}",
            e.line(),
            e.column()
        ))
    })?;
    let nachladen_bis = serde_json::from_slice::<PaketKopf>(&text)
        .unwrap_or_default()
        .nachladen_bis();
    let etag = header_etag.or_else(|| paket.stammdaten_etag.clone());
    Ok(PaketAntwort::Neu {
        paket: Box::new(paket),
        etag,
        nachladen_bis,
    })
}

/// Holt eine Seite Ereignisse mit Kern-`n > ab` (höchstens [`SEITE`]).
pub async fn ereignisse(basis: &str, token: &str, ab: u64) -> Result<EreignisSeite, ApiFehler> {
    let antwort = client(Duration::from_secs(10))?
        .get(format!("{basis}/turnier/ereignisse?ab={ab}&max={SEITE}"))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let text = koerper(antwort).await?;
    match serde_json::from_slice::<EreignisAntwort>(&text)
        .map_err(|_| ApiFehler::Format("Ereignisse".into()))?
    {
        EreignisAntwort::Liste(ereignisse) => Ok(EreignisSeite {
            ereignisse,
            kopf: None,
        }),
        EreignisAntwort::Objekt { ereignisse, n } => Ok(EreignisSeite {
            ereignisse,
            kopf: n,
        }),
    }
}

// ── Schreibende Aufrufe unter der Lease (Paket L3) ─────────────────────────

/// Relay-Angaben beim Nehmen der Lease: über diesen Namespace und Zugang
/// leitet badhub Eingaben aus dem Bereich „Spiele“ an diesen PC.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct RelayAngaben {
    pub ns: String,
    pub tl_token: String,
}

#[derive(Debug, Serialize)]
struct LeaseAnfrage<'a> {
    n: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    relay: Option<&'a RelayAngaben>,
    uebernehmen: bool,
}

/// Antwort auf `POST /turnier/lease`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseAntwort {
    /// `200 {epoche, n}`.
    Erteilt { epoche: u64, n: u64 },
    /// `409 n_veraltet {n}`: bts-light ist nicht auf dem Stand von badhub.
    NVeraltet { n: u64 },
    /// `409 lease_belegt {geraet}`: ein anderes Gerät hält die Lease.
    Belegt { geraet: String },
    /// Andere Ablehnung (Code aus `fehler`).
    Abgelehnt(String),
}

/// Antwort auf `POST /turnier/ereignisse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HochladenAntwort {
    /// `200 {bis_n}`.
    Bestaetigt { bis_n: u64 },
    /// `409 luecke {erwartet_n}`.
    Luecke { erwartet_n: u64 },
    /// `409 epoche_veraltet`: Steuerung in badhub zurückgeholt.
    EpocheVeraltet,
    /// `422 abweichung {posten, grund}`: badhub nimmt das Paket nicht an.
    Abweichung { grund: String },
    /// Standby-Upload angenommen (badhub S1, `200 {bis_n, epoche,
    /// abgewiesen}`): dieser PC hält jetzt die Lease unter `epoche`;
    /// `abgewiesen` Posten liegen in badhubs Ablage.
    StandbyUebernommen {
        bis_n: u64,
        epoche: u64,
        abgewiesen: u64,
    },
    /// `403 kein_standby`: badhub kennt dieses Gerät nicht als Standby der
    /// laufenden Lease.
    KeinStandby,
    /// `409 inhaber_aktiv {zuletzt_gesehen_vor_s}`: Der Hauptrechner war in
    /// den letzten 120 s bei badhub — badhub nimmt die Übernahme noch nicht
    /// an (nichts abgelegt, nichts geändert). Später erneut.
    InhaberAktiv { vor_s: u64 },
}

/// Antwort auf `DELETE /turnier/lease`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RueckgabeAntwort {
    Zurueckgegeben,
    /// `409 n_offen {n}`: badhub hat noch nicht alles.
    NOffen {
        n: u64,
    },
    EpocheVeraltet,
}

/// Fehlerkörper der badhub-API (`{fehler, …}`).
#[derive(Debug, Default, Deserialize)]
struct Fehlerkoerper {
    #[serde(default)]
    fehler: String,
    #[serde(default)]
    n: Option<u64>,
    #[serde(default)]
    erwartet_n: Option<u64>,
    #[serde(default)]
    geraet: Option<String>,
    #[serde(default)]
    grund: Option<String>,
    #[serde(default)]
    zuletzt_gesehen_vor_s: Option<u64>,
}

async fn fehlerkoerper(antwort: reqwest::Response) -> Fehlerkoerper {
    match koerper(antwort).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => Fehlerkoerper::default(),
    }
}

/// `POST /turnier/lease {n, relay?, uebernehmen: false}`.
pub async fn lease_nehmen(
    basis: &str,
    token: &str,
    n: u64,
    relay: Option<&RelayAngaben>,
) -> Result<LeaseAntwort, ApiFehler> {
    let body = serde_json::to_vec(&LeaseAnfrage {
        n,
        relay,
        uebernehmen: false,
    })
    .map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .post(format!("{basis}/turnier/lease"))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if status.is_success() {
        #[derive(Deserialize)]
        struct Ok_ {
            epoche: u64,
            n: u64,
        }
        let b = koerper(antwort).await?;
        let o: Ok_ = serde_json::from_slice(&b).map_err(|_| ApiFehler::Format("Lease".into()))?;
        return Ok(LeaseAntwort::Erteilt {
            epoche: o.epoche,
            n: o.n,
        });
    }
    if status.as_u16() == 409 {
        let f = fehlerkoerper(antwort).await;
        return Ok(match f.fehler.as_str() {
            "n_veraltet" => LeaseAntwort::NVeraltet {
                n: f.n.unwrap_or(0),
            },
            "lease_belegt" => LeaseAntwort::Belegt {
                geraet: f.geraet.unwrap_or_default(),
            },
            andere => LeaseAntwort::Abgelehnt(andere.to_string()),
        });
    }
    if status.as_u16() == 400 {
        let f = fehlerkoerper(antwort).await;
        return Ok(LeaseAntwort::Abgelehnt(f.fehler));
    }
    Err(status_fehler(status))
}

/// Antwort von `GET /turnier/lease` (badhub S1, nur lesen).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct LeaseSicht {
    #[serde(default)]
    pub lease: Option<super::paket::PaketLease>,
    #[serde(default)]
    pub n: u64,
}

/// `GET /turnier/lease`: Inhaber, Standby und die eigene Rolle.
pub async fn lease_lesen(basis: &str, token: &str) -> Result<LeaseSicht, ApiFehler> {
    let antwort = client(Duration::from_secs(8))?
        .get(format!("{basis}/turnier/lease"))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let b = koerper(antwort).await?;
    serde_json::from_slice(&b).map_err(|_| ApiFehler::Format("Lease".into()))
}

/// `POST /turnier/ereignisse {epoche, basis_n, posten}`. Mit `standby` (Paket S2)
/// zusätzlich `"standby": true` — der erste Upload nach einer
/// Standby-Übernahme ohne Netz. `posten` in der
/// Form aus [`super::protokoll::Posten::fuer_badhub`].
pub async fn hochladen(
    basis: &str,
    token: &str,
    epoche: u64,
    basis_n: u64,
    posten: &[serde_json::Value],
    standby: bool,
) -> Result<HochladenAntwort, ApiFehler> {
    let mut anfrage = serde_json::json!({
        "epoche": epoche,
        "basis_n": basis_n,
        "posten": posten,
    });
    if standby {
        anfrage["standby"] = serde_json::Value::Bool(true);
    }
    let body = serde_json::to_vec(&anfrage).map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(30))?
        .post(format!("{basis}/turnier/ereignisse"))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if status.is_success() {
        #[derive(Deserialize)]
        struct Ok_ {
            bis_n: u64,
            #[serde(default)]
            epoche: Option<u64>,
            #[serde(default)]
            abgewiesen: Option<u64>,
        }
        let b = koerper(antwort).await?;
        let o: Ok_ =
            serde_json::from_slice(&b).map_err(|_| ApiFehler::Format("Hochladen".into()))?;
        if standby {
            // Ohne neue Epoche wäre der Inhaberwechsel nicht belegt.
            let epoche = o
                .epoche
                .ok_or_else(|| ApiFehler::Format("Hochladen ohne Epoche".into()))?;
            return Ok(HochladenAntwort::StandbyUebernommen {
                bis_n: o.bis_n,
                epoche,
                abgewiesen: o.abgewiesen.unwrap_or(0),
            });
        }
        return Ok(HochladenAntwort::Bestaetigt { bis_n: o.bis_n });
    }
    match status.as_u16() {
        403 if standby => {
            let f = fehlerkoerper(antwort).await;
            if f.fehler == "kein_standby" {
                Ok(HochladenAntwort::KeinStandby)
            } else {
                Err(ApiFehler::Zugang)
            }
        }
        409 => {
            let f = fehlerkoerper(antwort).await;
            match f.fehler.as_str() {
                "luecke" => Ok(HochladenAntwort::Luecke {
                    erwartet_n: f.erwartet_n.unwrap_or(0),
                }),
                "epoche_veraltet" => Ok(HochladenAntwort::EpocheVeraltet),
                "inhaber_aktiv" if standby => Ok(HochladenAntwort::InhaberAktiv {
                    vor_s: f.zuletzt_gesehen_vor_s.unwrap_or(0),
                }),
                _ => Err(ApiFehler::Status(409)),
            }
        }
        422 => {
            let f = fehlerkoerper(antwort).await;
            Ok(HochladenAntwort::Abweichung {
                grund: f.grund.unwrap_or(f.fehler),
            })
        }
        _ => Err(status_fehler(status)),
    }
}

/// `DELETE /turnier/lease {epoche, n}`.
pub async fn lease_zurueckgeben(
    basis: &str,
    token: &str,
    epoche: u64,
    n: u64,
) -> Result<RueckgabeAntwort, ApiFehler> {
    let body = serde_json::to_vec(&serde_json::json!({ "epoche": epoche, "n": n }))
        .map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .delete(format!("{basis}/turnier/lease"))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if status.is_success() {
        return Ok(RueckgabeAntwort::Zurueckgegeben);
    }
    if status.as_u16() == 409 {
        let f = fehlerkoerper(antwort).await;
        return match f.fehler.as_str() {
            "n_offen" => Ok(RueckgabeAntwort::NOffen {
                n: f.n.unwrap_or(0),
            }),
            "epoche_veraltet" => Ok(RueckgabeAntwort::EpocheVeraltet),
            _ => Err(ApiFehler::Status(409)),
        };
    }
    Err(status_fehler(status))
}

// Debug von Hand: Zugänge erscheinen nie im Klartext (Security-Review L3).
impl std::fmt::Debug for KopplungAntwort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KopplungAntwort")
            .field("token", &"***")
            .field("turnier", &self.turnier)
            .finish()
    }
}

impl std::fmt::Debug for RelayAngaben {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayAngaben")
            .field("ns", &self.ns)
            .field("tl_token", &"***")
            .finish()
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn debug_zeigt_keine_zugaenge() {
        let r = RelayAngaben {
            ns: "ns-1".into(),
            tl_token: "geheim-relay".into(),
        };
        let k = KopplungAntwort {
            token: "geheim-geraet".into(),
            turnier: KopplungTurnier {
                id: 1,
                uuid: "u".into(),
                name: "n".into(),
            },
        };
        let text = format!("{r:?} {k:?}");
        assert!(!text.contains("geheim"), "{text}");
        assert!(text.contains("ns-1"));
    }
}
