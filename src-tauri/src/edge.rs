//! Gespräch mit dem Studio-Server (App „Kassenplätze“, /api/v1/kassenplaetze).
//!
//! Der Agent weist sich mit dem Gerätetoken aus. Die Oberfläche im Fenster bekommt es nie –
//! sie öffnet der Agent mit einem Einmal-Ticket.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::geraete::UsbGeraet;

#[derive(Debug)]
pub enum EdgeFehler {
    /// 401: Token ungültig – der Platz wurde entkoppelt oder neu gekoppelt
    NichtGekoppelt,
    /// Server nicht erreichbar
    Netz(String),
    /// Server antwortet mit Fehler
    Server(u16, String),
}

impl std::fmt::Display for EdgeFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeFehler::NichtGekoppelt => write!(f, "Der Platz ist nicht mehr gekoppelt."),
            EdgeFehler::Netz(m) => write!(f, "Studio-Server nicht erreichbar ({m})"),
            EdgeFehler::Server(c, m) => write!(f, "{m} (HTTP {c})"),
        }
    }
}

impl From<reqwest::Error> for EdgeFehler {
    fn from(e: reqwest::Error) -> Self {
        EdgeFehler::Netz(e.to_string())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Agentinfo {
    pub agent_version: String,
    pub system: String,
    pub rechner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hersteller: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seriennummer: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PlatzInfo {
    pub id: String,
    pub nr: String,
    pub name: String,
    pub art: String,
    pub ausrichtung: String,
    #[serde(default)]
    pub leerlauf: u32,
    /// Größe der Oberfläche in Prozent (Edge ab Kassenplätze Stufe 2; ältere schicken nichts)
    #[serde(default = "hundert")]
    pub zoom: u32,
    #[serde(default)]
    pub wartungs_pin: bool,
    #[serde(default)]
    pub usb: Vec<Value>,
    #[serde(default)]
    pub edge_version: String,
}

fn hundert() -> u32 {
    100
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EdgeKopf {
    pub tenant_id: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct KoppelAntwort {
    pub token: String,
    pub schluessel: String,
    pub edge: EdgeKopf,
    pub platz: PlatzInfo,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Auftrag {
    pub id: String,
    pub art: String,
    #[serde(default)]
    pub daten: Value,
}

#[derive(Debug, Serialize)]
pub struct Ergebnis {
    pub ok: bool,
    pub daten: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meldung: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
}

#[derive(Clone)]
pub struct Edge {
    pub basis: String,
    token: String,
    client: reqwest::Client,
}

fn client(zeit: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(zeit)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("StudioKassenplatz/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP-Client")
}

async fn antwort<T: for<'de> Deserialize<'de>>(r: reqwest::Response) -> Result<T, EdgeFehler> {
    let status = r.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(EdgeFehler::NichtGekoppelt);
    }
    if !status.is_success() {
        let text = r.text().await.unwrap_or_default();
        let meldung = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| match &v["detail"] {
                Value::String(s) => Some(s.clone()),
                Value::Object(o) => o.get("message").and_then(|m| m.as_str()).map(String::from),
                _ => None,
            })
            .unwrap_or(text);
        return Err(EdgeFehler::Server(status.as_u16(), meldung));
    }
    r.json::<T>().await.map_err(EdgeFehler::from)
}

fn api(basis: &str, pfad: &str) -> String {
    format!("{basis}/api/v1/kassenplaetze{pfad}")
}

/// Wer antwortet unter dieser Adresse? Ohne Anmeldung – für die Suche nach dem Server.
pub async fn kopf(basis: &str, zeit: Duration) -> Result<EdgeKopf, EdgeFehler> {
    antwort(client(zeit).get(api(basis, "/edge")).send().await?).await
}

pub async fn koppeln(basis: &str, code: &str, hardware_id: &str, info: &Agentinfo) -> Result<KoppelAntwort, EdgeFehler> {
    let mut body = serde_json::to_value(info).unwrap_or_default();
    body["code"] = code.into();
    body["hardware_id"] = hardware_id.into();
    antwort(client(Duration::from_secs(15)).post(api(basis, "/koppeln")).json(&body).send().await?).await
}

impl Edge {
    pub fn neu(basis: &str, token: &str) -> Self {
        Edge { basis: basis.to_string(), token: token.to_string(), client: client(Duration::from_secs(15)) }
    }

    fn mit(&self, r: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        r.bearer_auth(&self.token)
    }

    pub async fn herzschlag(&self, info: &Agentinfo, geraete: &[UsbGeraet]) -> Result<PlatzInfo, EdgeFehler> {
        let mut body = serde_json::to_value(info).unwrap_or_default();
        body["geraete"] = serde_json::to_value(geraete).unwrap_or_default();
        antwort(self.mit(self.client.post(api(&self.basis, "/ich/herzschlag"))).json(&body).send().await?).await
    }

    pub async fn ticket(&self) -> Result<String, EdgeFehler> {
        let v: Value = antwort(self.mit(self.client.post(api(&self.basis, "/ich/ticket"))).send().await?).await?;
        v["ticket"].as_str().map(String::from).ok_or(EdgeFehler::Server(500, "Ticket fehlt".into()))
    }

    /// Long-Poll: wartet bis zu 25 s auf Aufträge.
    pub async fn auftraege(&self) -> Result<Vec<Auftrag>, EdgeFehler> {
        let r = self
            .mit(client(Duration::from_secs(40)).get(api(&self.basis, "/ich/auftraege?warten=25")))
            .send()
            .await?;
        let v: Value = antwort(r).await?;
        Ok(serde_json::from_value(v["auftraege"].clone()).unwrap_or_default())
    }

    pub async fn ergebnis(&self, auftrag: &str, e: &Ergebnis) -> Result<(), EdgeFehler> {
        let _: Value = antwort(
            self.mit(self.client.post(api(&self.basis, &format!("/ich/auftraege/{auftrag}/ergebnis"))))
                .json(e)
                .send()
                .await?,
        )
        .await?;
        Ok(())
    }

    /// Ein Gerät meldet sich von selbst (Karte am Magnetkartenleser) – Inhalt verschlüsselt.
    pub async fn ereignis(&self, typ: &str, id: &str, daten: &str) -> Result<(), EdgeFehler> {
        let _: Value = antwort(
            self.mit(self.client.post(api(&self.basis, "/ich/ereignis")))
                .json(&serde_json::json!({ "typ": typ, "id": id, "daten": daten }))
                .send()
                .await?,
        )
        .await?;
        Ok(())
    }

    pub async fn wartung(&self, pin: &str) -> Result<bool, EdgeFehler> {
        let r = self
            .mit(self.client.post(api(&self.basis, "/ich/wartung")))
            .json(&serde_json::json!({ "pin": pin }))
            .send()
            .await?;
        match antwort::<Value>(r).await {
            Ok(_) => Ok(true),
            Err(EdgeFehler::Server(403, _)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn entkoppeln(&self) -> Result<(), EdgeFehler> {
        let _: Value = antwort(self.mit(self.client.post(api(&self.basis, "/ich/entkoppeln"))).send().await?).await?;
        Ok(())
    }

    /// Treiber und Herstellerbibliothek eines Geräts (ZIP mit pruefsummen.json).
    pub async fn geraetepaket(&self, typ: &str) -> Result<Vec<u8>, EdgeFehler> {
        let r = self
            .mit(client(Duration::from_secs(300)).get(api(&self.basis, &format!("/ich/geraetepakete/{typ}"))))
            .send()
            .await?;
        if r.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(EdgeFehler::NichtGekoppelt);
        }
        if !r.status().is_success() {
            let code = r.status().as_u16();
            let text = r.text().await.unwrap_or_default();
            let meldung = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["detail"].as_str().map(String::from))
                .unwrap_or(text);
            return Err(EdgeFehler::Server(code, meldung));
        }
        Ok(r.bytes().await?.to_vec())
    }
}
