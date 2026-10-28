//! Réglages utilisateur (UserData/settings.json). Jamais de mot de passe ici.

use serde::{Deserialize, Serialize};

use crate::paths::Layout;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMode {
    /// Serveur Erupe et PostgreSQL démarrés par le launcher sur ce PC.
    #[default]
    Local,
    /// Serveur Erupe déjà en marche ailleurs (autre PC, autre installation) : rien n'est démarré ici.
    Remote,
}

/// Adresse et ports du serveur. En local, `host` est l'adresse annoncée au jeu (127.0.0.1 en solo, l'adresse du
/// PC sur le réseau local pour accueillir d'autres joueurs) ; en distant, c'est l'adresse du serveur à joindre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    pub mode: ServerMode,
    pub host: String,
    pub api_port: u16,
    pub sign_port: u16,
    pub entrance_port: u16,
    /// Premier canal (monde débutants) ; le monde normal utilise le port suivant.
    pub channel_port: u16,
    pub db_port: u16,
}

impl Default for ServerSettings {
    fn default() -> Self {
        ServerSettings { mode: ServerMode::Local, host: "127.0.0.1".into(), api_port: 8080, sign_port: 53312, entrance_port: 53310, channel_port: 54001, db_port: 5433 }
    }
}

impl ServerSettings {
    pub fn is_local(&self) -> bool {
        self.mode == ServerMode::Local
    }

    pub fn channel_ports(&self) -> [u16; 2] {
        [self.channel_port, self.channel_port.saturating_add(1)]
    }

    /// Ports que le serveur local ouvre (PostgreSQL compris).
    pub fn local_ports(&self) -> Vec<u16> {
        let [c1, c2] = self.channel_ports();
        vec![self.db_port, self.api_port, self.sign_port, self.entrance_port, c1, c2]
    }

    /// Réglages incohérents, en clair ; vide si tout va bien.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let host = self.host.trim();
        if host.is_empty() {
            p.push("adresse du serveur vide".into());
        } else if !host.is_ascii() || host.contains(char::is_whitespace) {
            p.push("l'adresse du serveur ne doit contenir ni espace ni accent".into());
        }
        let ports = if self.is_local() { self.local_ports() } else { vec![self.api_port, self.entrance_port] };
        if ports.contains(&0) || self.channel_port == u16::MAX {
            p.push("port invalide".into());
        }
        let mut sorted = ports.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != ports.len() {
            p.push("deux services utilisent le même port".into());
        }
        p
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Dernier compte utilisé, pré-rempli à l'ouverture.
    pub last_account: String,
    /// Comptes déjà utilisés sur un serveur distant (en local, la liste vient de la base).
    pub remote_accounts: Vec<String>,
    pub server: ServerSettings,
    /// Langue de l'interface du launcher.
    pub ui_language: crate::i18n::Lang,
    /// Affichage et correctifs de compatibilité du jeu (onglet Jeu).
    pub game: crate::fixes::GameSettings,
}

impl Settings {
    pub fn load(layout: &Layout) -> Settings {
        // Fichier éditable à la main : un BOM UTF-8 (Bloc-notes, PowerShell) ne doit pas faire perdre les réglages.
        std::fs::read_to_string(layout.settings_file()).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or_default()
    }

    pub fn save(&self, layout: &Layout) {
        let _ = std::fs::create_dir_all(layout.user_data());
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(layout.settings_file(), text);
        }
    }
}
