//! Disposition des dossiers et ports du projet. Tout est relatif à la racine d'installation,
//! détectée depuis l'emplacement de l'exécutable (ou forcée par la variable FRONTIER_SOLO_ROOT).

use std::path::{Path, PathBuf};

pub const DB_NAME: &str = "erupe";
pub const DB_USER: &str = "postgres";

#[derive(Clone, Debug)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn detect() -> Layout {
        if let Ok(r) = std::env::var("FRONTIER_SOLO_ROOT") {
            return Layout { root: PathBuf::from(r) };
        }
        let exe = std::env::current_exe().ok();
        let mut dir = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        while let Some(d) = dir {
            if d.join("Server").join("erupe").is_dir() {
                return Layout { root: d };
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        Layout { root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) }
    }

    // Programme (remplaçable à la mise à jour)
    pub fn erupe_dir(&self) -> PathBuf { self.root.join("Server").join("erupe") }
    pub fn erupe_exe(&self) -> PathBuf { self.erupe_dir().join("erupe-ce.exe") }
    pub fn quest_bin(&self) -> PathBuf { self.root.join("Server").join("bin") }
    pub fn server_config(&self) -> PathBuf { self.root.join("Server").join("Config") }
    pub fn pgsql_bin(&self) -> PathBuf { self.root.join("Server").join("pgsql").join("bin") }
    pub fn mhf_iel_cli(&self) -> PathBuf { self.root.join("Tools").join("mhf-iel").join("mhf-iel-cli.exe") }
    pub fn default_client_dir(&self) -> PathBuf { self.root.join("Client") }

    // Données personnelles (jamais écrasées par une mise à jour)
    pub fn user_data(&self) -> PathBuf { self.root.join("UserData") }
    pub fn db_data(&self) -> PathBuf { self.user_data().join("Database") }
    pub fn credentials_file(&self) -> PathBuf { self.user_data().join("db-credentials.json") }
    pub fn settings_file(&self) -> PathBuf { self.user_data().join("settings.json") }
    pub fn screenshots(&self) -> PathBuf { self.user_data().join("Captures") }
    pub fn save_dumps(&self) -> PathBuf { self.user_data().join("Sauvegardes-personnages") }

    // Journaux
    pub fn logs(&self) -> PathBuf { self.root.join("Logs") }
    pub fn launcher_log(&self) -> PathBuf { self.logs().join("launcher.log") }
    pub fn pgsql_log(&self) -> PathBuf { self.logs().join("postgresql.log") }
    pub fn erupe_log(&self) -> PathBuf { self.logs().join("erupe.log") }

    /// Composants indispensables au serveur (état NON INSTALLÉ s'il en manque un).
    pub fn missing_server_components(&self) -> Vec<String> {
        let checks = [
            ("erupe-ce.exe", self.erupe_exe()),
            ("postgres.exe", self.pgsql_bin().join("postgres.exe")),
            ("pg_ctl.exe", self.pgsql_bin().join("pg_ctl.exe")),
            ("initdb.exe", self.pgsql_bin().join("initdb.exe")),
        ];
        checks.iter().filter(|(_, p)| !p.is_file()).map(|(n, _)| n.to_string()).collect()
    }

    /// État des autres éléments (affiché, sans bloquer le démarrage du serveur). Identifiants traduits par
    /// `i18n::Texts::component`.
    pub fn optional_components(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("quests", self.quest_bin().join("quests").is_dir()),
            ("client", self.default_client_dir().join("mhfo-hd.dll").is_file()),
            ("stub", self.mhf_iel_cli().is_file()),
        ]
    }

    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// Chemin au format attendu par config.json d'Erupe (séparateurs « / »).
    pub fn conf_path(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "/")
    }
}
