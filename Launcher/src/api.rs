//! API HTTP d'Erupe : santé, inscription, connexion, création de personnage.
//! Client HTTP/1.1 minimal sur TcpStream (l'API d'Erupe est en HTTP simple) : le launcher ne contient aucune
//! bibliothèque réseau. En mode local il ne parle qu'à 127.0.0.1 ; en mode distant, à l'adresse choisie.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::Deserialize;

use crate::settings::ServerSettings;

/// Adresse de l'API d'un serveur Erupe.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    /// En local, l'API est toujours jointe sur 127.0.0.1, même si une autre adresse est annoncée au jeu.
    pub fn of(server: &ServerSettings) -> Endpoint {
        let host = if server.is_local() { "127.0.0.1".to_string() } else { server.host.trim().to_string() };
        Endpoint { host, port: server.api_port }
    }
}

fn request(ep: &Endpoint, method: &str, path: &str, body: Option<&str>, token: Option<&str>) -> Result<(u16, String), String> {
    let addr = (ep.host.as_str(), ep.port)
        .to_socket_addrs()
        .map_err(|e| format!("adresse {}:{} introuvable ({e})", ep.host, ep.port))?
        .next()
        .ok_or_else(|| format!("adresse {}:{} introuvable", ep.host, ep.port))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| format!("serveur {}:{} injoignable ({e})", ep.host, ep.port))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let body = body.unwrap_or("");
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        ep.host,
        ep.port,
        body.len()
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let (head, payload) = text.split_once("\r\n\r\n").ok_or("réponse HTTP incomplète")?;
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or("statut HTTP illisible")?;
    let payload = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(payload) } else { payload.to_string() };
    Ok((status, payload))
}

fn dechunk(mut s: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = s.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 || rest.len() < n {
            break;
        }
        out.push_str(&rest[..n]);
        s = rest[n..].trim_start_matches("\r\n");
    }
    out
}

pub fn healthy(ep: &Endpoint) -> bool {
    matches!(request(ep, "GET", "/v2/health", None, None), Ok((200, _)))
}

/// Comme `healthy`, avec la raison de l'échec (mode distant : adresse fausse, serveur éteint…).
pub fn check(ep: &Endpoint) -> Result<(), String> {
    match request(ep, "GET", "/v2/health", None, None)? {
        (200, _) => Ok(()),
        (status, _) => Err(format!("{}:{} ne répond pas comme un serveur Erupe (HTTP {status})", ep.host, ep.port)),
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiUser {
    #[serde(rename = "tokenId")]
    pub token_id: u32,
    pub token: String,
    pub rights: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiCharacter {
    pub id: u32,
    pub name: String,
    pub hr: u32,
    pub gr: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MezFes {
    pub id: u32,
    pub start: u32,
    pub end: u32,
    #[serde(rename = "soloTickets")]
    pub solo_tickets: u32,
    #[serde(rename = "groupTickets")]
    pub group_tickets: u32,
    pub stalls: Vec<u32>,
}

/// Réponse de /v2/login et /v2/register : tout ce que le jeu attend de son launcher d'origine.
#[derive(Clone, Debug, Deserialize)]
pub struct AuthData {
    #[serde(rename = "currentTs")]
    pub current_ts: u32,
    #[serde(rename = "expiryTs")]
    pub expiry_ts: u32,
    #[serde(rename = "entranceCount")]
    pub entrance_count: u32,
    #[serde(default)]
    pub notices: Vec<String>,
    pub user: ApiUser,
    pub characters: Vec<ApiCharacter>,
    #[serde(rename = "mezFes")]
    pub mez_fes: Option<MezFes>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: String,
}

fn error_text(status: u16, payload: &str) -> String {
    let code = serde_json::from_str::<ApiError>(payload).map(|e| e.error).unwrap_or_default();
    match code.as_str() {
        "username_exists" => "ce nom de compte existe déjà".into(),
        "invalid_username" => "compte introuvable".into(),
        "invalid_password" => "mot de passe incorrect".into(),
        "missing_fields" => "nom de compte et mot de passe obligatoires".into(),
        _ => format!("erreur du serveur (HTTP {status}) {code}"),
    }
}

fn auth(ep: &Endpoint, path: &str, user: &str, pass: &str) -> Result<AuthData, String> {
    let body = serde_json::json!({ "username": user, "password": pass }).to_string();
    let (status, payload) = request(ep, "POST", path, Some(&body), None)?;
    if status != 200 {
        return Err(error_text(status, &payload));
    }
    serde_json::from_str(&payload).map_err(|e| format!("réponse inattendue du serveur : {e}"))
}

pub fn register(ep: &Endpoint, user: &str, pass: &str) -> Result<AuthData, String> {
    auth(ep, "/v2/register", user, pass)
}

pub fn login(ep: &Endpoint, user: &str, pass: &str) -> Result<AuthData, String> {
    auth(ep, "/v2/login", user, pass)
}

/// Réserve un emplacement de personnage ; le jeu lance ensuite sa propre création (apparence, nom).
pub fn create_character(ep: &Endpoint, token: &str) -> Result<ApiCharacter, String> {
    let (status, payload) = request(ep, "POST", "/v2/characters", None, Some(token))?;
    if status != 200 {
        return Err(error_text(status, &payload));
    }
    serde_json::from_str(&payload).map_err(|e| format!("réponse inattendue du serveur : {e}"))
}
