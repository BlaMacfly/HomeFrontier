//! Compte SQL local (UserData/db-credentials.json), créé au premier démarrage.

use serde::{Deserialize, Serialize};

use crate::paths::{DB_USER, Layout};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub user: String,
    pub password: String,
}

impl Credentials {
    pub fn load(layout: &Layout) -> Option<Credentials> {
        let text = std::fs::read_to_string(layout.credentials_file()).ok()?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    }

    /// Identifiants existants, ou nouveaux identifiants enregistrés avant l'initialisation de la base. Un fichier
    /// présent mais illisible n'est jamais remplacé : la base a été créée avec ce mot de passe.
    pub fn load_or_create(layout: &Layout) -> std::io::Result<Credentials> {
        if let Some(c) = Credentials::load(layout) {
            return Ok(c);
        }
        if layout.credentials_file().exists() {
            return Err(std::io::Error::other(format!("{} est illisible (corrigez-le ou restaurez-le depuis une sauvegarde)", layout.credentials_file().display())));
        }
        let c = Credentials { user: DB_USER.into(), password: crate::util::random_password(24) };
        std::fs::create_dir_all(layout.user_data())?;
        std::fs::write(layout.credentials_file(), serde_json::to_string_pretty(&c).unwrap_or_default())?;
        Ok(c)
    }
}
