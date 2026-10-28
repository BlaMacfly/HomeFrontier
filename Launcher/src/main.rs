//! HomeFrontier — launcher (Rust, egui). © 2026 BlaMacfly.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod client;
mod credentials;
mod db;
mod frontier_text;
mod i18n;
mod orchestrator;
mod paths;
mod server;
mod settings;
mod ui;
mod util;

use eframe::egui;

fn main() -> eframe::Result {
    let layout = paths::Layout::detect();
    // Un seul launcher par installation : un second arrêterait les services du premier.
    let _instance_lock = {
        use std::os::windows::fs::OpenOptionsExt;
        let _ = std::fs::create_dir_all(layout.user_data());
        match std::fs::OpenOptions::new().create(true).write(true).share_mode(0).open(layout.user_data().join(".launcher.lock")) {
            Ok(f) => Some(f),
            // ERROR_SHARING_VIOLATION : un autre launcher de cette installation est ouvert.
            Err(e) if e.raw_os_error() == Some(32) => {
                eprintln!("{}", i18n::FR.already_running);
                return Ok(());
            }
            Err(_) => None,
        }
    };
    let (shared, tx, orchestrator_thread) = orchestrator::spawn(layout.clone());

    // Options de test (Tests/) : --start démarre les services, --quit-after N ferme après N secondes,
    // --do acccreate <compte> <mot de passe> crée un compte une fois le serveur prêt,
    // --do play <compte> <mot de passe> [n° de personnage] se connecte et lance le jeu (sans n° : nouveau personnage).
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--start") {
        let _ = tx.send(orchestrator::Command::StartAll);
    }
    let quit_after = args.iter().position(|a| a == "--quit-after").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<u64>().ok());
    // --tab jouer|comptes|serveur : onglet affiché à l'ouverture (captures d'écran des tests).
    let start_tab = args.iter().position(|a| a == "--tab").and_then(|i| args.get(i + 1)).cloned();
    if let Some(i) = args.iter().position(|a| a == "--do") {
        match (args.get(i + 1).map(String::as_str), args.get(i + 2), args.get(i + 3)) {
            (Some("acccreate"), Some(u), Some(p)) => {
                let _ = tx.send(orchestrator::Command::CreateAccount { user: u.clone(), pass: p.clone() });
            }
            (Some("translate"), _, _) => {
                let _ = tx.send(orchestrator::Command::ApplyTranslation);
            }
            (Some("untranslate"), _, _) => {
                let _ = tx.send(orchestrator::Command::RestoreOriginalText);
            }
            // Outil : --do quest-dump <tsv> <première quête> <dernière quête> (sans démarrer le serveur).
            (Some("quest-dump"), Some(out), Some(from)) => {
                let to = args.get(i + 4).and_then(|v| v.parse().ok()).unwrap_or(0);
                let dir = layout.quest_bin().join("quests");
                let backup = frontier_text::quest_backup_dir(&layout);
                match frontier_text::quest::dump_range(&dir, &backup, from.parse().unwrap_or(0), to, std::path::Path::new(out)) {
                    Ok(n) => println!("{n} quête(s) écrite(s) dans {out}"),
                    Err(e) => eprintln!("ERREUR : {e}"),
                }
                return Ok(());
            }
            (Some("quest-translate"), _, _) => {
                let _ = tx.send(orchestrator::Command::ApplyQuestTranslation);
            }
            (Some("quest-untranslate"), _, _) => {
                let _ = tx.send(orchestrator::Command::RestoreQuests);
            }
            (Some("quest-inventory"), Some(out), _) => {
                let _ = tx.send(orchestrator::Command::QuestInventory(std::path::PathBuf::from(out)));
            }
            (Some("translate-verify"), Some(text), _) => {
                let _ = tx.send(orchestrator::Command::VerifyTranslation(text.clone()));
            }
            (Some("play"), Some(u), Some(p)) => {
                let _ = tx.send(orchestrator::Command::Login { user: u.clone(), pass: p.clone() });
                let character = args.get(i + 4).and_then(|c| c.parse().ok());
                let _ = tx.send(orchestrator::Command::Play { character });
            }
            _ => {}
        }
    }

    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1240.0, 760.0]).with_min_inner_size([900.0, 640.0]).with_title(i18n::FR.window_title);
    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/logo-512.png")) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        viewport = viewport.with_icon(std::sync::Arc::new(egui::IconData { rgba: rgba.into_raw(), width, height }));
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let tx_ui = tx.clone();
    let result = eframe::run_native(
        i18n::FR.window_title,
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, layout, shared, tx_ui, quit_after, start_tab)))),
    );
    // Fermeture : l'orchestrateur arrête Erupe puis PostgreSQL ; on attend qu'il ait fini.
    let _ = tx.send(orchestrator::Command::Shutdown);
    // Large : une traduction des quêtes en cours (plusieurs minutes) doit pouvoir se terminer.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
    while !orchestrator_thread.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    result
}
