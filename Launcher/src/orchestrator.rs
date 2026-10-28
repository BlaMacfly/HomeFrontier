//! Machine à états et thread de pilotage : l'interface envoie des commandes, l'orchestrateur démarre et
//! arrête PostgreSQL et Erupe dans le bon ordre (mode local) ou vérifie le serveur choisi (mode distant), et
//! publie l'état dans `Shared`.

use std::process::Child;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::api::{AuthData, Endpoint};
use crate::credentials::Credentials;
use crate::paths::Layout;
use crate::settings::Settings;
use crate::{api, client, db, frontier_text, server, util};

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    NotInstalled(Vec<String>),
    Stopped,
    Initializing,
    StartingDb,
    StartingServer,
    Connecting,
    Ready,
    Playing,
    Stopping,
    Error(String),
}

pub enum Command {
    StartAll,
    StopAll,
    CreateAccount { user: String, pass: String },
    /// Connexion au serveur : jeton de session et liste des personnages.
    Login { user: String, pass: String },
    /// Lance le jeu avec ce personnage (None : nouveau personnage, créé par le jeu).
    Play { character: Option<u32> },
    /// Relit la liste des comptes (base locale, ou comptes déjà utilisés en mode distant).
    RefreshAccounts,
    /// Nouveaux réglages (serveur arrêté uniquement).
    ApplySettings(Settings),
    /// Langue de l'interface (enregistrée dans les réglages, à tout moment).
    SetUiLanguage(crate::i18n::Lang),
    /// Applique la traduction française aux fichiers texte du client (jeu fermé uniquement).
    ApplyTranslation,
    /// Remet les fichiers texte d'origine du client.
    RestoreOriginalText,
    /// Tests : cherche une chaîne dans les fichiers texte décodés du client.
    VerifyTranslation(String),
    /// Outil de traduction : inventaire des textes de quêtes du serveur, écrit en TSV.
    QuestInventory(std::path::PathBuf),
    /// Applique `Localization/quetes-fr.tsv` aux quêtes du serveur.
    ApplyQuestTranslation,
    /// Remet les quêtes d'origine.
    RestoreQuests,
    /// Outil de traduction : inventaire des textes des scénarios du serveur, écrit en TSV.
    ScenarioInventory(std::path::PathBuf),
    /// Applique `Localization/scenarios-fr.tsv` et `scenarios-fr-auto.tsv` aux scénarios du serveur.
    ApplyScenarioTranslation,
    /// Remet les scénarios d'origine.
    RestoreScenarios,
    Shutdown,
}

#[derive(Default)]
pub struct Shared {
    pub state: Option<State>,
    pub log: Vec<String>,
    pub busy: bool,
    /// Session ouverte (compte, personnages) ; effacée à l'arrêt du serveur.
    pub session: Option<(String, AuthData)>,
    /// Comptes connus : nom et nombre de personnages (None : inconnu, serveur distant).
    pub accounts: Vec<(String, Option<u32>)>,
    pub settings: Settings,
    pub translation: Option<frontier_text::Status>,
    /// Quêtes du serveur traduites (nombre de fichiers) et présence du fichier de traductions.
    pub quests: (usize, bool),
    /// Scénarios du serveur traduits (nombre de fichiers) et présence d'un fichier de traductions.
    pub scenarios: (usize, bool),
}

pub type SharedRef = Arc<Mutex<Shared>>;

struct Orchestrator {
    layout: Layout,
    shared: SharedRef,
    settings: Settings,
    erupe: Option<Child>,
    game: Option<Child>,
    /// Serveur distant joignable (mode distant uniquement).
    remote_ready: bool,
    /// PostgreSQL démarré par ce launcher (on n'arrête pas une base qu'on a trouvée déjà en marche… sauf la nôtre).
    db_started: bool,
}

impl Orchestrator {
    fn set_state(&self, s: State) {
        if let Ok(mut sh) = self.shared.lock() {
            sh.state = Some(s);
        }
    }

    fn log(&self, msg: impl Into<String>) {
        let line = format!("[{}] {}", util::timestamp(), msg.into());
        let _ = std::fs::create_dir_all(self.layout.logs());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.layout.launcher_log()) {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
        if let Ok(mut sh) = self.shared.lock() {
            sh.log.push(line);
            let excess = sh.log.len().saturating_sub(500);
            sh.log.drain(..excess);
        }
    }

    fn set_busy(&self, b: bool) {
        if let Ok(mut sh) = self.shared.lock() {
            sh.busy = b;
        }
    }

    fn api(&self) -> Endpoint {
        Endpoint::of(&self.settings.server)
    }

    /// Le serveur (local démarré ou distant joint) accepte-t-il des connexions ?
    fn online(&self) -> bool {
        if self.settings.server.is_local() { self.erupe.is_some() } else { self.remote_ready }
    }

    fn save_settings(&self) {
        self.settings.save(&self.layout);
        if let Ok(mut sh) = self.shared.lock() {
            sh.settings = self.settings.clone();
        }
    }

    fn idle_state(&self) -> State {
        if !self.settings.server.is_local() {
            return State::Stopped;
        }
        let missing = self.layout.missing_server_components();
        if missing.is_empty() { State::Stopped } else { State::NotInstalled(missing) }
    }

    fn start_all(&mut self) {
        if self.online() {
            return;
        }
        let problems = self.settings.server.problems();
        if !problems.is_empty() {
            self.log(format!("Réglages du serveur incorrects : {}", problems.join(" ; ")));
            return;
        }
        if !self.settings.server.is_local() {
            self.connect_remote();
            return;
        }
        if let State::NotInstalled(m) = self.idle_state() {
            self.log(format!("Composants manquants : {}", m.join(", ")));
            return;
        }
        if let Err(e) = self.try_start() {
            self.log(format!("ERREUR : {e}"));
            self.set_state(State::Error(e));
            self.stop_all_quiet();
        }
        self.refresh_accounts();
    }

    fn connect_remote(&mut self) {
        let ep = self.api();
        self.set_state(State::Connecting);
        self.log(format!("Connexion au serveur distant {}:{}…", ep.host, ep.port));
        match api::check(&ep) {
            Ok(()) => {
                self.remote_ready = true;
                self.set_state(State::Ready);
                self.log("Serveur distant joignable");
            }
            Err(e) => {
                self.log(format!("ERREUR : {e}"));
                self.set_state(State::Error(e));
            }
        }
    }

    fn try_start(&mut self) -> Result<(), String> {
        let server = self.settings.server.clone();
        let creds = Credentials::load_or_create(&self.layout).map_err(|e| format!("identifiants SQL : {e}"))?;
        if !db::is_initialized(&self.layout) {
            self.set_state(State::Initializing);
            self.log("Premier démarrage : initialisation de la base PostgreSQL…");
            db::initialize(&self.layout, &creds)?;
        }
        self.set_state(State::StartingDb);
        let already = db::is_running(&self.layout);
        self.log(if already { "PostgreSQL déjà en marche (repris)" } else { "Démarrage de PostgreSQL…" });
        db::start(&self.layout, server.db_port)?;
        self.db_started = true;
        if db::ensure_database(&self.layout, &creds, server.db_port)? {
            self.log("Base « erupe » créée (Erupe appliquera son schéma au démarrage)");
        }

        self.set_state(State::StartingServer);
        let busy = server::busy_ports(&server);
        if !busy.is_empty() {
            return Err(format!("ports déjà utilisés : {busy:?} (un autre serveur est-il ouvert ? les ports se changent dans l'onglet Serveur)"));
        }
        server::generate_config(&self.layout, &creds, &server)?;
        if !self.layout.quest_bin().join("quests").is_dir() {
            self.log("Attention : fichiers de quêtes absents (Server/bin) ; le jeu plantera à l'entrée en quête");
        }
        self.log(format!("Démarrage d'Erupe (adresse annoncée {}, entrance {})…", server.host.trim(), server.entrance_port));
        self.erupe = Some(server::start(&self.layout, &server)?);
        self.set_state(State::Ready);
        self.log("Serveur prêt");
        Ok(())
    }

    fn stop_all(&mut self) {
        self.set_state(State::Stopping);
        self.stop_all_quiet();
        self.set_state(self.idle_state());
    }

    fn stop_all_quiet(&mut self) {
        if let Ok(mut sh) = self.shared.lock() {
            sh.session = None;
        }
        if let Some(mut game) = self.game.take() {
            // Le jeu est fermé avant le serveur : Erupe sauvegarde le personnage à la déconnexion.
            self.log("Fermeture du jeu…");
            let _ = game.kill();
            let _ = game.wait();
        }
        if self.remote_ready {
            self.remote_ready = false;
            self.log("Déconnecté du serveur distant");
        }
        if let Some(mut child) = self.erupe.take() {
            self.log("Arrêt d'Erupe…");
            if server::stop(&mut child) {
                self.log("Erupe arrêté proprement");
            } else {
                self.log("Erupe ne répondait pas : arrêt forcé");
            }
        }
        if self.db_started || (self.layout.pgsql_bin().is_dir() && db::is_running(&self.layout)) {
            self.log("Arrêt de PostgreSQL…");
            match db::stop(&self.layout) {
                Ok(()) => self.log("PostgreSQL arrêté"),
                Err(e) => self.log(format!("ERREUR : {e}")),
            }
            self.db_started = false;
        }
    }

    fn refresh_accounts(&mut self) {
        let accounts = if self.settings.server.is_local() {
            if self.erupe.is_none() {
                return;
            }
            let Some(creds) = Credentials::load(&self.layout) else { return };
            match db::accounts(&self.layout, &creds, self.settings.server.db_port) {
                Ok(list) => list.into_iter().map(|(n, c)| (n, Some(c))).collect(),
                Err(e) => {
                    self.log(format!("Liste des comptes illisible : {e}"));
                    return;
                }
            }
        } else {
            self.settings.remote_accounts.iter().map(|n| (n.clone(), None)).collect()
        };
        if let Ok(mut sh) = self.shared.lock() {
            sh.accounts = accounts;
        }
    }

    fn remember_account(&mut self, user: &str) {
        self.settings.last_account = user.to_string();
        if !self.settings.server.is_local() && !self.settings.remote_accounts.iter().any(|a| a == user) {
            self.settings.remote_accounts.push(user.to_string());
        }
        self.save_settings();
    }

    fn create_account(&mut self, user: &str, pass: &str) {
        if !self.online() {
            self.log("Création de compte impossible : le serveur n'est pas démarré");
            return;
        }
        match api::register(&self.api(), user, pass) {
            Ok(_) => {
                self.log(format!("Compte « {user} » créé"));
                self.remember_account(user);
            }
            Err(e) => self.log(format!("Compte « {user} » non créé : {e}")),
        }
        self.refresh_accounts();
    }

    fn login(&mut self, user: &str, pass: &str) {
        if !self.online() {
            self.log("Connexion impossible : le serveur n'est pas démarré");
            return;
        }
        match api::login(&self.api(), user, pass) {
            Ok(auth) => {
                self.log(format!("Connecté au compte « {user} » ({} personnage(s))", auth.characters.len()));
                self.remember_account(user);
                if let Ok(mut sh) = self.shared.lock() {
                    sh.session = Some((user.to_string(), auth));
                }
            }
            Err(e) => self.log(format!("Connexion refusée : {e}")),
        }
        self.refresh_accounts();
    }

    fn apply_settings(&mut self, new: Settings) {
        if self.online() || self.game.is_some() {
            self.log("Arrêtez le serveur avant de changer ses réglages");
            return;
        }
        let problems = new.server.problems();
        if !problems.is_empty() {
            self.log(format!("Réglages refusés : {}", problems.join(" ; ")));
            return;
        }
        let mode_changed = new.server.mode != self.settings.server.mode;
        self.settings = new;
        self.save_settings();
        let s = &self.settings.server;
        self.log(if s.is_local() {
            format!("Réglages enregistrés : serveur local, adresse annoncée {}, API {}, sign {}, entrance {}, canaux {}-{}, base {}", s.host.trim(), s.api_port, s.sign_port, s.entrance_port, s.channel_port, s.channel_port.saturating_add(1), s.db_port)
        } else {
            format!("Réglages enregistrés : serveur distant {} (API {}, entrance {})", s.host.trim(), s.api_port, s.entrance_port)
        });
        if mode_changed {
            if let Ok(mut sh) = self.shared.lock() {
                sh.accounts.clear();
            }
            self.refresh_accounts();
        }
        self.set_state(self.idle_state());
    }

    fn play(&mut self, character: Option<u32>) {
        if self.game.is_some() {
            self.log("Le jeu est déjà lancé");
            return;
        }
        let Some((user, auth)) = self.shared.lock().ok().and_then(|sh| sh.session.clone()) else {
            self.log("Connectez-vous d'abord à un compte");
            return;
        };
        let client_dir = self.layout.default_client_dir();
        let problems = client::check(&self.layout, &client_dir);
        if !problems.is_empty() {
            self.log(format!("Lancement impossible : {}", problems.join(" ; ")));
            return;
        }
        let (ch, is_new) = match character {
            Some(id) => match auth.characters.iter().find(|c| c.id == id) {
                // Emplacement réservé dont la création n'a jamais été terminée en jeu : on la relance.
                Some(c) => (c.clone(), c.name.is_empty()),
                None => {
                    self.log("Personnage introuvable");
                    return;
                }
            },
            None => match api::create_character(&self.api(), &auth.user.token) {
                Ok(c) => {
                    self.log(format!("Emplacement de personnage créé (n° {})", c.id));
                    (c, true)
                }
                Err(e) => {
                    self.log(format!("Création du personnage refusée : {e}"));
                    return;
                }
            },
        };
        self.log(format!("Lancement du jeu : compte « {user} », {}", if is_new { "nouveau personnage".to_string() } else { format!("personnage « {} »", ch.name) }));
        match client::launch(&self.layout, &client_dir, &self.settings.server, &auth, &ch, is_new) {
            Ok(child) => {
                self.game = Some(child);
                self.set_state(State::Playing);
            }
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
    }

    fn refresh_translation(&self) {
        let status = frontier_text::status(&self.layout, &self.layout.default_client_dir());
        let quests = frontier_text::quest_status(&self.layout);
        let scenarios = frontier_text::scenario_status(&self.layout);
        if let Ok(mut sh) = self.shared.lock() {
            sh.translation = Some(status);
            sh.quests = quests;
            sh.scenarios = scenarios;
        }
    }

    fn apply_translation(&self) {
        if self.game.is_some() {
            self.log("Fermez le jeu avant de traduire ses fichiers");
            return;
        }
        self.log("Application de la traduction française (sauvegarde des fichiers d'origine au premier passage)…");
        let started = std::time::Instant::now();
        match frontier_text::apply(&self.layout, &self.layout.default_client_dir()) {
            Ok(applied) => {
                let total: usize = applied.files.iter().map(|(_, n)| n).sum();
                for (file, n) in &applied.files {
                    self.log(format!("  {file} : {n} chaîne(s) traduite(s)"));
                }
                if !applied.skipped.is_empty() {
                    self.log(format!("  Sections pas encore prises en charge (restent en anglais) : {}", applied.skipped.join(", ")));
                }
                self.log(format!("Traduction appliquée : {total} chaîne(s) dans {} fichier(s), en {} s", applied.files.len(), started.elapsed().as_secs()));
            }
            Err(e) => {
                self.log(format!("ERREUR : traduction non appliquée ({e}) ; restauration des fichiers d'origine"));
                if let Err(e) = frontier_text::restore(&self.layout, &self.layout.default_client_dir()) {
                    self.log(format!("ERREUR : restauration impossible ({e})"));
                }
            }
        }
        self.refresh_translation();
    }

    fn apply_quest_translation(&self) {
        if self.game.is_some() {
            self.log("Fermez le jeu avant de traduire les quêtes");
            return;
        }
        let (translations, manual_n, auto_n) = match frontier_text::quest_translations(&self.layout) {
            Ok(t) => t,
            Err(e) => {
                self.log(format!("ERREUR : traductions de quêtes illisibles ({e})"));
                return;
            }
        };
        self.log(format!("Traduction des quêtes : {} texte(s) traduit(s) disponibles ({manual_n} manuels, {auto_n} automatiques)…", translations.len()));
        let started = std::time::Instant::now();
        match frontier_text::quest::apply_dir(&self.layout.quest_bin().join("quests"), &frontier_text::quest_backup_dir(&self.layout), &translations) {
            Ok(r) => {
                self.log(format!("Quêtes traduites : {} fichier(s) modifié(s) sur {}, {} texte(s) remplacé(s), en {} s", r.changed, r.files, r.texts, started.elapsed().as_secs()));
                if self.erupe.is_some() {
                    self.log("Redémarrez le serveur pour qu'Erupe recharge les quêtes");
                }
            }
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
        self.refresh_translation();
    }

    fn restore_quests(&self) {
        if self.game.is_some() {
            self.log("Fermez le jeu avant de restaurer les quêtes");
            return;
        }
        match frontier_text::quest::restore_dir(&self.layout.quest_bin().join("quests"), &frontier_text::quest_backup_dir(&self.layout)) {
            Ok(n) => self.log(format!("Quêtes d'origine restaurées ({n} fichier(s))")),
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
        self.refresh_translation();
    }

    fn write_inventory(&self, label: &str, inv: Result<frontier_text::quest::Inventory, String>, out: &std::path::Path, names: &[&str], started: std::time::Instant) {
        match inv {
            Ok(inv) => {
                let occurrences: usize = inv.texts.values().map(|(n, _)| n).sum();
                self.log(format!(
                    "{label} : {} fichier(s), {} illisible(s), {} texte(s) uniques pour {occurrences} occurrence(s), en {} s",
                    inv.files,
                    inv.unreadable.len(),
                    inv.texts.len(),
                    started.elapsed().as_secs()
                ));
                if !inv.unreadable.is_empty() {
                    self.log(format!("  illisibles (premiers) : {}", inv.unreadable.iter().take(10).cloned().collect::<Vec<_>>().join(", ")));
                }
                match frontier_text::quest::write_inventory_tsv(&inv, out, names) {
                    Ok(()) => self.log(format!("  inventaire écrit : {}", out.display())),
                    Err(e) => self.log(format!("ERREUR : {e}")),
                }
            }
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
    }

    fn apply_scenario_translation(&self) {
        if self.game.is_some() {
            self.log("Fermez le jeu avant de traduire les scénarios");
            return;
        }
        let (translations, manual_n, auto_n) = match frontier_text::scenario_translations(&self.layout) {
            Ok(t) => t,
            Err(e) => {
                self.log(format!("ERREUR : traductions de scénarios illisibles ({e})"));
                return;
            }
        };
        self.log(format!("Traduction des scénarios : {} texte(s) traduit(s) disponibles ({manual_n} manuels, {auto_n} automatiques)…", translations.len()));
        let started = std::time::Instant::now();
        match frontier_text::scenario::apply_dir(&self.layout.quest_bin().join("scenarios"), &frontier_text::scenario_backup_dir(&self.layout), &translations) {
            Ok(r) => {
                self.log(format!("Scénarios traduits : {} fichier(s) modifié(s) sur {}, {} texte(s) remplacé(s), en {} s", r.changed, r.files, r.texts, started.elapsed().as_secs()));
                if self.erupe.is_some() {
                    self.log("Redémarrez le serveur pour qu'Erupe recharge les scénarios");
                }
            }
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
        self.refresh_translation();
    }

    fn restore_scenarios(&self) {
        if self.game.is_some() {
            self.log("Fermez le jeu avant de restaurer les scénarios");
            return;
        }
        match frontier_text::scenario::restore_dir(&self.layout.quest_bin().join("scenarios"), &frontier_text::scenario_backup_dir(&self.layout)) {
            Ok(n) => self.log(format!("Scénarios d'origine restaurés ({n} fichier(s))")),
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
        self.refresh_translation();
    }

    fn restore_original_text(&self) {
        if self.game.is_some() {
            self.log("Fermez le jeu avant de restaurer ses fichiers");
            return;
        }
        match frontier_text::restore(&self.layout, &self.layout.default_client_dir()) {
            Ok(n) => self.log(format!("Fichiers texte d'origine restaurés ({n} fichier(s))")),
            Err(e) => self.log(format!("ERREUR : {e}")),
        }
        self.refresh_translation();
    }

    /// Surveillance : le jeu ou Erupe se sont-ils arrêtés tout seuls ?
    fn watch(&mut self) {
        if let Some(game) = self.game.as_mut() {
            if let Ok(Some(status)) = game.try_wait() {
                self.game = None;
                self.log(format!("Fin du jeu : {}", client::exit_text(status.code())));
                // Jeton à usage unique côté jeu : une nouvelle connexion sera demandée pour relancer.
                if let Ok(mut sh) = self.shared.lock() {
                    sh.session = None;
                }
                if self.online() {
                    self.set_state(State::Ready);
                }
                self.refresh_accounts();
            }
        }
        if let Some(child) = self.erupe.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                self.erupe = None;
                let msg = format!("Erupe s'est arrêté de lui-même (code {:?}, voir {})", status.code(), self.layout.erupe_log().display());
                self.log(format!("ERREUR : {msg}"));
                self.set_state(State::Error(msg));
            }
        }
    }
}

pub fn spawn(layout: Layout) -> (SharedRef, Sender<Command>, JoinHandle<()>) {
    let settings = Settings::load(&layout);
    let shared: SharedRef = Arc::new(Mutex::new(Shared { settings: settings.clone(), ..Default::default() }));
    let (tx, rx): (Sender<Command>, Receiver<Command>) = channel();
    let mut orch = Orchestrator { layout, shared: shared.clone(), settings, erupe: None, game: None, remote_ready: false, db_started: false };
    let handle = std::thread::spawn(move || {
        orch.set_state(orch.idle_state());
        orch.log(format!("Launcher {} — racine {}", Layout::version(), orch.layout.root.display()));
        orch.refresh_accounts();
        orch.refresh_translation();
        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(cmd) => {
                    orch.set_busy(true);
                    match cmd {
                        Command::StartAll => orch.start_all(),
                        Command::StopAll => orch.stop_all(),
                        Command::CreateAccount { user, pass } => orch.create_account(&user, &pass),
                        Command::Login { user, pass } => orch.login(&user, &pass),
                        Command::Play { character } => orch.play(character),
                        Command::RefreshAccounts => orch.refresh_accounts(),
                        Command::ApplySettings(s) => orch.apply_settings(s),
                        Command::SetUiLanguage(lang) => {
                            orch.settings.ui_language = lang;
                            orch.save_settings();
                        }
                        Command::ApplyTranslation => orch.apply_translation(),
                        Command::RestoreOriginalText => orch.restore_original_text(),
                        Command::ApplyQuestTranslation => orch.apply_quest_translation(),
                        Command::RestoreQuests => orch.restore_quests(),
                        Command::QuestInventory(out) => {
                            let started = std::time::Instant::now();
                            let inv = frontier_text::quest::inventory(&orch.layout.quest_bin().join("quests"), &frontier_text::quest_backup_dir(&orch.layout));
                            orch.write_inventory("Quêtes", inv, &out, &frontier_text::quest::FIELD_NAMES, started);
                        }
                        Command::ScenarioInventory(out) => {
                            let started = std::time::Instant::now();
                            let inv = frontier_text::scenario::inventory(&orch.layout.quest_bin().join("scenarios"), &frontier_text::scenario_backup_dir(&orch.layout));
                            orch.write_inventory("Scénarios", inv, &out, &frontier_text::scenario::FIELD_NAMES, started);
                        }
                        Command::ApplyScenarioTranslation => orch.apply_scenario_translation(),
                        Command::RestoreScenarios => orch.restore_scenarios(),
                        Command::VerifyTranslation(needle) => {
                            for (file, r) in frontier_text::verify(&orch.layout.default_client_dir(), &needle) {
                                orch.log(match r {
                                    Ok((size, found)) => format!("  {file} : décodé ({size} octets), « {needle} » {}", if found { "TROUVÉ" } else { "absent" }),
                                    Err(e) => format!("  {file} : ERREUR de décodage ({e})"),
                                });
                            }
                        }
                        Command::Shutdown => {
                            orch.stop_all();
                            orch.set_busy(false);
                            return;
                        }
                    }
                    orch.set_busy(false);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => orch.watch(),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    orch.stop_all();
                    return;
                }
            }
        }
    });
    (shared, tx, handle)
}
