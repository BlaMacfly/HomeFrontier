//! Erupe : génération de config.json, démarrage, attente de l'API, arrêt propre par Ctrl+Pause.
//! config.json est entièrement généré par le launcher : modifier ce fichier-ci, pas Server/Config/config.json.

use std::process::Child;
use std::time::{Duration, Instant};

use serde_json::json;
use windows_sys::Win32::System::Console::{AttachConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler};

use crate::api::Endpoint;
use crate::credentials::Credentials;
use crate::paths::{DB_NAME, Layout};
use crate::settings::ServerSettings;
use crate::util::{hidden_group, port_open};

pub fn generate_config(layout: &Layout, creds: &Credentials, server: &ServerSettings) -> Result<(), String> {
    let host = server.host.trim();
    let [channel_1, channel_2] = server.channel_ports();
    let config = json!({
        // Adresse annoncée au jeu pour l'entrance et les canaux (127.0.0.1 en solo).
        "Host": host,
        "BinPath": Layout::conf_path(&layout.quest_bin()),
        "Language": "fr",
        "ClientMode": "ZZ",
        "AutoCreateAccount": false,
        // Solo : ni compte à rebours ni attente des autres joueurs à l'arrêt.
        "DisableShutdownCountdown": true,
        "ShutdownDrainSeconds": 0,
        "Screenshots": { "Enabled": true, "Host": host, "Port": server.api_port, "OutputDir": Layout::conf_path(&layout.screenshots()), "UploadQuality": 100 },
        "SaveDumps": { "Enabled": true, "RawEnabled": false, "OutputDir": Layout::conf_path(&layout.save_dumps()) },
        "Database": { "Host": "127.0.0.1", "Port": server.db_port, "User": creds.user, "Password": creds.password, "Database": DB_NAME },
        "Sign": { "Enabled": true, "Port": server.sign_port },
        "API": { "Enabled": true, "Port": server.api_port, "LandingPage": { "Enabled": false, "Title": "", "Content": "" } },
        "Channel": { "Enabled": true },
        // Le client décide seul des rangs admis selon le type de monde : un personnage débutant est refusé sur
        // un monde « Normal » (type 1), d'où un monde « Newbie » (type 3) comme dans la configuration d'Erupe.
        // Noms en ASCII : ils sont envoyés en Shift-JIS.
        "Entrance": { "Enabled": true, "Port": server.entrance_port, "Entries": [
            { "Name": "HomeFrontier Debutants", "Description": "", "IP": "", "Type": 3, "Recommended": 2, "AllowedClientFlags": 0,
              "Channels": [ { "Port": channel_1, "MaxPlayers": 4, "Enabled": true } ] },
            { "Name": "HomeFrontier", "Description": "", "IP": "", "Type": 1, "Recommended": 0, "AllowedClientFlags": 0,
              "Channels": [ { "Port": channel_2, "MaxPlayers": 4, "Enabled": true } ] }
        ] }
    });
    std::fs::create_dir_all(layout.server_config()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(layout.server_config().join("config.json"), text).map_err(|e| e.to_string())
}

/// Ports qu'Erupe va ouvrir et qui sont déjà pris (un autre serveur, ou un Erupe resté en marche).
pub fn busy_ports(server: &ServerSettings) -> Vec<u16> {
    let [c1, c2] = server.channel_ports();
    [server.sign_port, server.entrance_port, c1, c2, server.api_port].into_iter().filter(|p| port_open(*p)).collect()
}

/// Démarre Erupe (dossier courant : Server/Config, où il lit config.json) et attend que son API réponde.
pub fn start(layout: &Layout, server: &ServerSettings) -> Result<Child, String> {
    let api = Endpoint::of(server);
    std::fs::create_dir_all(layout.logs()).map_err(|e| e.to_string())?;
    let log = std::fs::File::create(layout.erupe_log()).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    let mut child = hidden_group(&layout.erupe_exe())
        .current_dir(layout.server_config())
        .stdout(log)
        .stderr(log_err)
        .spawn()
        .map_err(|e| format!("erupe-ce.exe n'a pas pu être lancé : {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("Erupe s'est arrêté au démarrage (code {:?}, voir {})", status.code(), layout.erupe_log().display()));
        }
        if crate::api::healthy(&api) && port_open(server.channel_port) {
            return Ok(child);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err(format!("Erupe ne répond pas (voir {})", layout.erupe_log().display()));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Envoie Ctrl+Pause à la console cachée d'Erupe : Go le reçoit comme une interruption et Erupe déconnecte
/// les joueurs (sauvegarde des personnages) avant de fermer ses serveurs.
fn send_ctrl_break(pid: u32) -> bool {
    // SAFETY : appels Win32 sans pointeur ; la console du launcher (s'il en a une) est détachée le temps de
    // s'attacher à celle d'Erupe, et le launcher ignore l'événement pendant ce temps.
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            return false;
        }
        SetConsoleCtrlHandler(None, 1);
        let sent = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) != 0;
        FreeConsole();
        std::thread::sleep(Duration::from_millis(200));
        SetConsoleCtrlHandler(None, 0);
        sent
    }
}

/// Arrêt propre, puis forcé au bout de 45 s. Renvoie vrai si l'arrêt a été propre.
pub fn stop(child: &mut Child) -> bool {
    if let Ok(Some(_)) = child.try_wait() {
        return true;
    }
    if send_ctrl_break(child.id()) {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}
