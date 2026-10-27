//! Client MHF : contrôle du dossier, configuration du boot stub mhf-iel et lancement du jeu.
//! Le launcher (64 bits) ne peut pas charger mhfo-hd.dll (32 bits) : il écrit la configuration attendue par
//! mhf-iel et lance `Tools/mhf-iel/mhf-iel-cli.exe`, qui charge le moteur du jeu dans son propre processus.
//! Le mot de passe n'est jamais transmis au jeu : seul le jeton de session obtenu par l'API l'est.

use std::path::{Path, PathBuf};
use std::process::Child;

use serde_json::json;

use crate::api::{ApiCharacter, AuthData};
use crate::paths::Layout;
use crate::settings::ServerSettings;

/// Stands de la MezFes : mhf-iel attend le nom de la variante, Erupe envoie son numéro.
fn mez_stall_name(id: u32) -> Option<&'static str> {
    Some(match id {
        2 => "TokotokoPartnya",
        3 => "Pachinko",
        4 => "VolpakkunTogether",
        5 => "GoocooScoop",
        6 => "Nyanrendo",
        7 => "HoneyPanic",
        8 => "DokkanBattleCats",
        9 => "PointStall",
        10 => "StallMap",
        _ => return None,
    })
}

/// Problèmes qui empêchent le lancement, en clair ; vide si le client est utilisable.
pub fn check(layout: &Layout, client_dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    if !client_dir.join("mhfo-hd.dll").is_file() && !client_dir.join("mhfo.dll").is_file() {
        problems.push(format!("client introuvable dans {} (mhfo-hd.dll absent)", client_dir.display()));
    }
    // Le moteur reçoit son chemin en octets ANSI (CP932) : tout caractère hors ASCII risque d'être illisible.
    if !client_dir.to_string_lossy().is_ascii() {
        problems.push(format!("le chemin du client contient des accents ou caractères spéciaux : {}", client_dir.display()));
    }
    if !layout.mhf_iel_cli().is_file() {
        problems.push(format!("programme de lancement absent : {}", layout.mhf_iel_cli().display()));
    }
    problems
}

/// Configuration de mhf-iel (`MhfConfig`), noms de champs identiques à ceux de sa désérialisation.
fn game_config(client_dir: &Path, server: &ServerSettings, auth: &AuthData, ch: &ApiCharacter, is_new: bool) -> serde_json::Value {
    let mez = auth.mez_fes.as_ref();
    json!({
        "char_id": ch.id,
        "char_name": ch.name,
        "char_gr": ch.gr,
        "char_hr": ch.hr,
        "char_ids": auth.characters.iter().map(|c| c.id).chain(is_new.then_some(ch.id)).collect::<Vec<_>>(),
        // Nouveau personnage : le jeu lance sa propre création, sinon il demanderait une sauvegarde inexistante.
        "char_new": is_new,
        "user_token_id": auth.user.token_id,
        "user_token": auth.user.token,
        "user_name": "",
        "user_password": "",
        "user_rights": auth.user.rights,
        // Le jeu rejoint l'entrance du serveur choisi ; mhf-iel en déduit aussi l'API (même hôte).
        "server_host": if server.is_local() { "127.0.0.1" } else { server.host.trim() },
        "server_port": server.entrance_port,
        "entrance_count": auth.entrance_count,
        "current_ts": auth.current_ts,
        "expiry_ts": auth.expiry_ts,
        "notices": auth.notices.iter().map(|n| json!({ "flags": 0, "data": n })).collect::<Vec<_>>(),
        "mez_event_id": mez.map_or(0, |m| m.id),
        "mez_start": mez.map_or(0, |m| m.start),
        "mez_end": mez.map_or(0, |m| m.end),
        "mez_solo_tickets": mez.map_or(0, |m| m.solo_tickets),
        "mez_group_tickets": mez.map_or(0, |m| m.group_tickets),
        "mez_stalls": mez.map_or_else(Vec::new, |m| m.stalls.iter().filter_map(|id| mez_stall_name(*id)).collect()),
        "version": "ZZ",
        "mhf_folder": client_dir,
    })
}

pub fn config_file(layout: &Layout) -> PathBuf {
    layout.user_data().join("mhf-iel-config.json")
}

/// Écrit la configuration (jeton compris, fichier personnel dans UserData) et lance le boot stub.
pub fn launch(layout: &Layout, client_dir: &Path, server: &ServerSettings, auth: &AuthData, ch: &ApiCharacter, is_new: bool) -> Result<Child, String> {
    let problems = check(layout, client_dir);
    if !problems.is_empty() {
        return Err(problems.join(" ; "));
    }
    let config = serde_json::to_string_pretty(&game_config(client_dir, server, auth, ch, is_new)).map_err(|e| e.to_string())?;
    std::fs::write(config_file(layout), config).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(layout.logs()).map_err(|e| e.to_string())?;
    let log = std::fs::File::create(layout.logs().join("mhf-iel.log")).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    std::process::Command::new(layout.mhf_iel_cli())
        .arg("--config-file")
        .arg(config_file(layout))
        .current_dir(client_dir)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(log_err)
        .spawn()
        .map_err(|e| format!("mhf-iel-cli.exe n'a pas pu être lancé : {e}"))
}

/// Code de sortie de mhf-iel-cli, en clair.
pub fn exit_text(code: Option<i32>) -> String {
    match code {
        Some(0) => "jeu fermé".into(),
        Some(1) => "arguments refusés par mhf-iel-cli".into(),
        Some(2) => "configuration de lancement introuvable".into(),
        Some(3) => "configuration de lancement illisible (voir Logs/mhf-iel.log)".into(),
        Some(4) => "le moteur du jeu n'a pas pu être chargé (voir Logs/mhf-iel.log)".into(),
        Some(101) => "mhf-iel-cli a planté (voir Logs/mhf-iel.log)".into(),
        Some(c) => format!("le jeu s'est terminé avec le code {c}"),
        None => "le jeu a été interrompu".into(),
    }
}
