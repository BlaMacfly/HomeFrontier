//! Interface egui : affiche en fond, colonne de gauche (en-tête logo + titre, onglets Jouer / Comptes /
//! Serveur, journal).

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::i18n::{self, Lang};
use crate::orchestrator::{Command, SharedRef, State};
use crate::paths::Layout;
use crate::settings::{ServerMode, ServerSettings};

const PANEL_WIDTH: f32 = 420.0;
const VEIL: egui::Color32 = egui::Color32::from_rgba_premultiplied(8, 8, 12, 210);
const GOLD: egui::Color32 = egui::Color32::from_rgb(236, 176, 72);
const GOLD_DARK: egui::Color32 = egui::Color32::from_rgb(150, 92, 24);
const OK_GREEN: egui::Color32 = egui::Color32::from_rgb(120, 200, 120);
const KO_RED: egui::Color32 = egui::Color32::from_rgb(230, 110, 90);
const TITLE_FONT: &str = "cinzel";

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Play,
    Accounts,
    Server,
    Translation,
}

pub struct App {
    layout: Layout,
    shared: SharedRef,
    tx: Sender<Command>,
    background: Option<egui::TextureHandle>,
    logo: Option<egui::TextureHandle>,
    optional: Vec<(&'static str, bool)>,
    /// Langue de l'interface (sélecteur de l'en-tête).
    lang: Lang,
    tab: Tab,
    account_name: String,
    account_pass: String,
    /// Personnage choisi : None = nouveau personnage.
    character: Option<u32>,
    new_name: String,
    new_pass: String,
    new_pass_confirm: String,
    /// Réglages du serveur en cours d'édition (onglet Serveur).
    server_draft: Option<ServerSettings>,
    quit_at: Option<Instant>,
    closing: bool,
}

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

/// Polices japonaises de Windows essayées, dans l'ordre, pour l'interface en japonais (egui n'a pas ces glyphes).
/// Rien n'est intégré à l'exécutable : sans aucune de ces polices, le japonais s'affiche en carrés.
const JAPANESE_SYSTEM_FONTS: [&str; 4] = ["YuGothM.ttc", "YuGothR.ttc", "meiryo.ttc", "msgothic.ttc"];

/// Cinzel (SIL OFL 1.1, assets/fonts/OFL-Cinzel.txt) pour le titre ; police d'egui pour le reste, complétée par une
/// police japonaise du système en secours.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(TITLE_FONT.into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Cinzel.ttf"))));
    fonts.families.insert(egui::FontFamily::Name(TITLE_FONT.into()), vec![TITLE_FONT.into()]);
    let windir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
    if let Some(bytes) = JAPANESE_SYSTEM_FONTS.iter().find_map(|f| std::fs::read(windir.join("Fonts").join(f)).ok()) {
        fonts.font_data.insert("japonais".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("japonais".into());
        }
    }
    ctx.set_fonts(fonts);
}

/// Étendue horizontale du logo « Monster Hunter Frontier Z » dans l'affiche (fractions de sa largeur).
const POSTER_LOGO_U: (f32, f32) = (0.26, 0.74);

/// Place l'affiche en une seule image continue (pas de second exemplaire sous la colonne) : elle couvre toute la
/// fenêtre sans déformation, agrandie si besoin pour que son logo tienne à droite de la colonne, et décalée pour
/// centrer ce logo dans la zone visible. Le rectangle renvoyé peut dépasser de la fenêtre (rogné à l'affichage).
fn poster_rect(tex: [usize; 2], full: egui::Rect, column_right: f32) -> egui::Rect {
    let ratio = tex[0] as f32 / tex[1].max(1) as f32;
    let (lu0, lu1) = POSTER_LOGO_U;
    let margin = 24.0;
    // Couvrir la fenêtre, et assez large pour que le logo entier passe dans la zone de droite.
    let visible = (full.right() - column_right - 2.0 * margin).max(1.0);
    let mut width = (full.width()).max(full.height() * ratio).max(visible / (lu1 - lu0));
    width = width.min(visible / (lu1 - lu0) * 1.6).max(full.width()).max(full.height() * ratio);
    let height = width / ratio;
    // Centre du logo au centre de la zone de droite, sans laisser de bande vide à gauche ni à droite.
    let target_center = (column_right + full.right()) / 2.0;
    let left = (target_center - width * (lu0 + lu1) / 2.0).clamp(full.right() - width, full.left());
    let top = full.top() + (full.height() - height) / 2.0;
    egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height))
}

fn big_button(text: &str, fill: egui::Color32) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).size(17.0).strong().color(egui::Color32::WHITE)).fill(fill).min_size(egui::vec2(PANEL_WIDTH - 32.0, 40.0))
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title).size(15.0).strong().color(GOLD));
    ui.add_space(2.0);
}

/// Champ de saisie à largeur fixe (tient dans la colonne) et fond lisible sur le voile sombre.
fn field(text: &mut String, password: bool) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).password(password).desired_width(210.0).background_color(egui::Color32::from_rgba_premultiplied(30, 30, 38, 235))
}

fn port_field(ui: &mut egui::Ui, label: &str, value: &mut u16) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(1..=65535).speed(1.0));
    ui.end_row();
}

impl App {
    pub fn new(cc: &eframe::CreationContext, layout: Layout, shared: SharedRef, tx: Sender<Command>, quit_after: Option<u64>, start_tab: Option<String>) -> App {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        install_fonts(&cc.egui_ctx);
        let optional = layout.optional_components();
        let settings = crate::settings::Settings::load(&layout);
        let (account_name, lang) = (settings.last_account, settings.ui_language);
        App {
            lang,
            // fond.jpg hors du dépôt (logo Capcom) : intégré seulement s'il est présent à la compilation (build.rs).
            #[cfg(has_fond)]
            background: load_texture(&cc.egui_ctx, "fond", include_bytes!("../assets/fond.jpg")),
            #[cfg(not(has_fond))]
            background: None,
            logo: load_texture(&cc.egui_ctx, "logo", include_bytes!("../assets/logo-512.png")),
            layout,
            shared,
            tx,
            optional,
            tab: match start_tab.as_deref() {
                Some("comptes") => Tab::Accounts,
                Some("serveur") => Tab::Server,
                Some("traduction") => Tab::Translation,
                _ => Tab::Play,
            },
            account_name,
            account_pass: String::new(),
            character: None,
            new_name: String::new(),
            new_pass: String::new(),
            new_pass_confirm: String::new(),
            server_draft: None,
            quit_at: quit_after.map(|s| Instant::now() + Duration::from_secs(s)),
            closing: false,
        }
    }

    /// En-tête : logo en grand, titre « HOMEFRONTIER » en Cinzel doré avec ombre portée, sous-titre.
    fn header(&mut self, ui: &mut egui::Ui) {
        let t = i18n::texts(self.lang);
        // Sélecteur de langue de l'interface, en haut à droite de la colonne.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let before = self.lang;
            egui::ComboBox::from_id_salt("langue")
                .selected_text(self.lang.native_name())
                .width(96.0)
                .show_ui(ui, |ui| {
                    for lang in Lang::ALL {
                        ui.selectable_value(&mut self.lang, lang, lang.native_name());
                    }
                });
            ui.small(egui::RichText::new(t.language).color(egui::Color32::from_gray(160)));
            if self.lang != before {
                let _ = self.tx.send(Command::SetUiLanguage(self.lang));
            }
        });
        let t = i18n::texts(self.lang);
        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(112.0, 112.0)));
            }
            let font = egui::FontId::new(38.0, egui::FontFamily::Name(TITLE_FONT.into()));
            let galley = ui.painter().layout_no_wrap(t.window_title.to_uppercase(), font, GOLD);
            let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
            let shadow = ui.painter().layout_no_wrap(t.window_title.to_uppercase(), egui::FontId::new(38.0, egui::FontFamily::Name(TITLE_FONT.into())), egui::Color32::from_black_alpha(220));
            ui.painter().galley(rect.min + egui::vec2(2.0, 2.0), shadow, egui::Color32::BLACK);
            ui.painter().galley(rect.min, galley, GOLD);
            let y = rect.bottom() + 3.0;
            ui.painter().hline(egui::Rangef::new(rect.left() + 20.0, rect.right() - 20.0), y, egui::Stroke::new(1.5_f32, GOLD_DARK));
            ui.add_space(8.0);
            ui.label(egui::RichText::new(t.subtitle).italics().color(egui::Color32::from_gray(200)));
            ui.small(egui::RichText::new(format!("v{}", Layout::version())).color(egui::Color32::from_gray(140)));
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        let t = i18n::texts(self.lang);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (tab, label) in [(Tab::Play, t.tab_play), (Tab::Accounts, t.tab_accounts), (Tab::Server, t.tab_server), (Tab::Translation, t.tab_translation)] {
                let selected = self.tab == tab;
                let text = egui::RichText::new(label).size(15.0).color(if selected { GOLD } else { egui::Color32::from_gray(190) });
                if ui.add(egui::Button::new(text).selected(selected).min_size(egui::vec2((PANEL_WIDTH - 56.0) / 4.0, 28.0))).clicked() {
                    self.tab = tab;
                    if tab == Tab::Accounts {
                        let _ = self.tx.send(Command::RefreshAccounts);
                    }
                }
            }
        });
        ui.separator();
    }

    fn status(&self, ui: &mut egui::Ui, state: &State, busy: bool) {
        let t = i18n::texts(self.lang);
        ui.horizontal(|ui| {
            ui.strong(format!("{} :", t.state_label));
            let color = match state {
                State::Ready | State::Playing => OK_GREEN,
                State::Error(_) | State::NotInstalled(_) => KO_RED,
                _ => egui::Color32::from_gray(210),
            };
            ui.colored_label(color, t.state(state));
            if busy {
                ui.spinner();
            }
        });
        if self.closing {
            ui.label(t.closing);
        }
    }

    fn tab_play(&mut self, ui: &mut egui::Ui, state: &State, busy: bool, session: Option<&(String, crate::api::AuthData)>, server: &ServerSettings) {
        let t = i18n::texts(self.lang);
        self.status(ui, state, busy);
        ui.small(if server.is_local() {
            format!("{} — {}", t.mode_local, server.host.trim())
        } else {
            format!("{} — {}:{}", t.mode_remote, server.host.trim(), server.api_port)
        });
        ui.add_space(6.0);
        let online = matches!(state, State::Ready | State::Playing);
        let can_start = !busy && matches!(state, State::Stopped | State::Error(_));
        if !online {
            let label = if server.is_local() { t.start } else { t.connect };
            if ui.add_enabled(can_start, big_button(label, egui::Color32::from_rgb(70, 90, 120))).clicked() {
                let _ = self.tx.send(Command::StartAll);
            }
        } else if ui.add_enabled(!busy, egui::Button::new(if server.is_local() { t.stop } else { t.disconnect })).clicked() {
            let _ = self.tx.send(Command::StopAll);
        }

        match session {
            None => {
                section(ui, t.account);
                egui::Grid::new("compte").num_columns(2).show(ui, |ui| {
                    ui.label(t.account_name);
                    ui.add(field(&mut self.account_name, false));
                    ui.end_row();
                    ui.label(t.account_pass);
                    ui.add(field(&mut self.account_pass, true));
                    ui.end_row();
                });
                let filled = !self.account_name.trim().is_empty() && !self.account_pass.is_empty();
                let login = ui.add_enabled(online && !busy && filled, egui::Button::new(t.login));
                if !online {
                    login.on_disabled_hover_text(t.account_needs_server);
                } else if login.clicked() {
                    let _ = self.tx.send(Command::Login { user: self.account_name.trim().to_string(), pass: std::mem::take(&mut self.account_pass) });
                }
                ui.small(t.accounts_hint);
            }
            Some((user, auth)) => {
                section(ui, &format!("{} : {user}", t.account));
                ui.label(t.character);
                if self.character.is_some_and(|id| !auth.characters.iter().any(|c| c.id == id)) {
                    self.character = None;
                }
                if self.character.is_none() {
                    self.character = auth.characters.first().map(|c| c.id);
                }
                for c in &auth.characters {
                    let name = if c.name.is_empty() { t.unnamed.to_string() } else { c.name.clone() };
                    ui.radio_value(&mut self.character, Some(c.id), format!("{name} — HR {} / GR {}", c.hr, c.gr));
                }
                ui.radio_value(&mut self.character, None, t.new_character);
                ui.add_space(8.0);
                let can_play = !busy && matches!(state, State::Ready);
                if ui.add_enabled(can_play, big_button(t.play, GOLD_DARK)).clicked() {
                    let _ = self.tx.send(Command::Play { character: self.character });
                }
            }
        }

        section(ui, t.components);
        for (name, ok) in &self.optional {
            ui.colored_label(if *ok { OK_GREEN } else { KO_RED }, format!("{} : {}", t.component(name), if *ok { t.present } else { t.missing }));
        }
    }

    fn tab_accounts(&mut self, ui: &mut egui::Ui, state: &State, busy: bool, accounts: &[(String, Option<u32>)], server: &ServerSettings) {
        let t = i18n::texts(self.lang);
        let online = matches!(state, State::Ready | State::Playing);
        section(ui, t.accounts_list);
        if accounts.is_empty() {
            ui.label(if server.is_local() && !online { t.accounts_need_server } else { t.accounts_empty });
        }
        egui::Grid::new("comptes").num_columns(3).striped(true).show(ui, |ui| {
            for (name, chars) in accounts {
                ui.label(egui::RichText::new(name).strong());
                ui.label(match chars {
                    Some(n) => format!("{n} {}", t.characters),
                    None => String::new(),
                });
                if ui.button(t.use_account).clicked() {
                    self.account_name = name.clone();
                    self.account_pass.clear();
                    self.tab = Tab::Play;
                }
                ui.end_row();
            }
        });

        section(ui, t.new_account);
        egui::Grid::new("nouveau").num_columns(2).show(ui, |ui| {
            ui.label(t.account_name);
            ui.add(field(&mut self.new_name, false));
            ui.end_row();
            ui.label(t.account_pass);
            ui.add(field(&mut self.new_pass, true));
            ui.end_row();
            ui.label(t.account_pass_confirm);
            ui.add(field(&mut self.new_pass_confirm, true));
            ui.end_row();
        });
        let name = self.new_name.trim().to_string();
        let mismatch = !self.new_pass_confirm.is_empty() && self.new_pass != self.new_pass_confirm;
        if mismatch {
            ui.colored_label(KO_RED, t.pass_mismatch);
        }
        let valid = !name.is_empty() && name.is_ascii() && !self.new_pass.is_empty() && self.new_pass == self.new_pass_confirm;
        if !name.is_empty() && !name.is_ascii() {
            ui.colored_label(KO_RED, t.name_ascii);
        }
        let create = ui.add_enabled(online && !busy && valid, egui::Button::new(t.create));
        if !online {
            create.on_disabled_hover_text(t.account_needs_server);
        } else if create.clicked() {
            let pass = std::mem::take(&mut self.new_pass);
            self.new_pass_confirm.clear();
            let _ = self.tx.send(Command::CreateAccount { user: name.clone(), pass });
            self.account_name = name;
            self.new_name.clear();
        }
    }

    fn tab_server(&mut self, ui: &mut egui::Ui, state: &State, busy: bool, current: &ServerSettings) {
        let t = i18n::texts(self.lang);
        let online = matches!(state, State::Ready | State::Playing);
        let draft = self.server_draft.get_or_insert_with(|| current.clone());
        section(ui, t.server_mode);
        ui.add_enabled_ui(!online && !busy, |ui| {
            ui.radio_value(&mut draft.mode, ServerMode::Local, t.mode_local_long);
            ui.radio_value(&mut draft.mode, ServerMode::Remote, t.mode_remote_long);
            section(ui, t.server_address);
            egui::Grid::new("serveur").num_columns(2).show(ui, |ui| {
                ui.label(if draft.is_local() { t.host_local } else { t.host_remote });
                ui.add(field(&mut draft.host, false));
                ui.end_row();
                port_field(ui, t.port_api, &mut draft.api_port);
                port_field(ui, t.port_entrance, &mut draft.entrance_port);
                if draft.is_local() {
                    port_field(ui, t.port_sign, &mut draft.sign_port);
                    port_field(ui, t.port_channel, &mut draft.channel_port);
                    port_field(ui, t.port_db, &mut draft.db_port);
                }
            });
            ui.small(if draft.is_local() { t.host_local_hint } else { t.host_remote_hint });
        });
        let problems = draft.problems();
        for p in &problems {
            ui.colored_label(KO_RED, p);
        }
        if online {
            ui.colored_label(GOLD, t.server_stop_first);
        }
        let changed = draft != current;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!online && !busy && changed && problems.is_empty(), egui::Button::new(t.save)).clicked() {
                let mut settings = self.shared.lock().map(|sh| sh.settings.clone()).unwrap_or_default();
                settings.server = draft.clone();
                settings.server.host = settings.server.host.trim().to_string();
                let _ = self.tx.send(Command::ApplySettings(settings));
            }
            if ui.add_enabled(!online && !busy && changed, egui::Button::new(t.cancel)).clicked() {
                *draft = current.clone();
            }
            if ui.add_enabled(!online && !busy, egui::Button::new(t.defaults)).clicked() {
                let mode = draft.mode;
                *draft = ServerSettings { mode, ..Default::default() };
            }
        });
    }
}

impl App {
    fn tab_translation(&mut self, ui: &mut egui::Ui, state: &State, busy: bool, status: Option<&crate::frontier_text::Status>, quests: (usize, bool)) {
        let t = i18n::texts(self.lang);
        use crate::frontier_text::Status;
        section(ui, t.translation_state);
        let (text, color) = match status {
            Some(Status::Translated) => (t.translation_on, OK_GREEN),
            Some(Status::Original) => (t.translation_off, GOLD),
            Some(Status::NoPayload) => (t.translation_no_payload, KO_RED),
            None => ("…", egui::Color32::from_gray(190)),
        };
        ui.colored_label(color, text);
        ui.add_space(6.0);
        ui.label(t.translation_about);
        ui.small(t.translation_accents);
        ui.small(t.translation_server_text);
        ui.add_space(8.0);
        let playing = matches!(state, State::Playing);
        let can_apply = !busy && !playing && !matches!(status, Some(Status::NoPayload));
        let apply = ui.add_enabled(can_apply, big_button(t.translation_apply, GOLD_DARK));
        if playing {
            apply.on_disabled_hover_text(t.translation_close_game);
        } else if apply.clicked() {
            let _ = self.tx.send(Command::ApplyTranslation);
        }
        ui.add_space(4.0);
        if ui.add_enabled(!busy && !playing && matches!(status, Some(Status::Translated)), egui::Button::new(t.translation_restore)).clicked() {
            let _ = self.tx.send(Command::RestoreOriginalText);
        }

        // Quêtes envoyées par le serveur (Localization/quetes-fr.tsv).
        let (translated_quests, has_file) = quests;
        section(ui, t.quests_state);
        if translated_quests > 0 {
            ui.colored_label(OK_GREEN, format!("{translated_quests} {}", t.quests_on));
        } else {
            ui.colored_label(GOLD, t.quests_off);
        }
        if !has_file {
            ui.colored_label(KO_RED, t.quests_no_file);
        }
        ui.small(t.quests_about);
        ui.horizontal(|ui| {
            let apply = ui.add_enabled(!busy && !playing && has_file, egui::Button::new(t.quests_apply));
            if apply.clicked() {
                let _ = self.tx.send(Command::ApplyQuestTranslation);
            }
            if ui.add_enabled(!busy && !playing && translated_quests > 0, egui::Button::new(t.quests_restore)).clicked() {
                let _ = self.tx.send(Command::RestoreQuests);
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = i18n::texts(self.lang);
        ctx.request_repaint_after(Duration::from_millis(250));
        if self.quit_at.is_some_and(|t| Instant::now() >= t) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            // La fenêtre se ferme ; main() attend l'arrêt des services par l'orchestrateur.
            self.closing = true;
        }
        let (state, busy, log, session, accounts, settings, translation, quests) = {
            let sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (sh.state.clone().unwrap_or(State::Stopped), sh.busy, sh.log.clone(), sh.session.clone(), sh.accounts.clone(), sh.settings.clone(), sh.translation.clone(), sh.quests)
        };
        // Réglages enregistrés par l'orchestrateur : le brouillon de l'onglet Serveur repart de ceux-ci.
        if self.tab != Tab::Server {
            self.server_draft = None;
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let full = ui.max_rect();
            // Colonne de gauche (en-tête, onglets, journal) sur voile sombre, par-dessus une affiche unique placée par
            // `poster_rect` pour que son logo reste entier à droite quelle que soit la taille de la fenêtre.
            let column = egui::Rect::from_min_max(full.min, egui::pos2(full.left() + PANEL_WIDTH, full.bottom()));
            if let Some(tex) = &self.background {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().with_clip_rect(full).image(tex.id(), poster_rect(tex.size(), full, column.right()), uv, egui::Color32::WHITE);
            }
            ui.painter().rect_filled(column, 0.0, VEIL);
            ui.painter().vline(column.right(), column.y_range(), egui::Stroke::new(2.0_f32, GOLD_DARK));

            let journal_rect = egui::Rect::from_min_max(egui::pos2(column.left(), column.bottom() - 150.0), column.max);
            ui.painter().hline(journal_rect.x_range().shrink(16.0_f32), journal_rect.top(), egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)));
            let mut journal_ui = ui.new_child(egui::UiBuilder::new().max_rect(journal_rect.shrink(12.0)));
            journal_ui.strong(t.journal);
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(&mut journal_ui, |ui| {
                for line in &log {
                    ui.label(egui::RichText::new(line).small().color(egui::Color32::from_gray(190)));
                }
            });

            let panel_rect = egui::Rect::from_min_max(column.min, egui::pos2(column.right(), journal_rect.top()));
            let mut panel_ui = ui.new_child(egui::UiBuilder::new().max_rect(panel_rect.shrink(16.0)));
            self.header(&mut panel_ui);
            self.tabs(&mut panel_ui);
            egui::ScrollArea::vertical().auto_shrink(false).show(&mut panel_ui, |ui| match self.tab {
                Tab::Play => self.tab_play(ui, &state, busy, session.as_ref(), &settings.server),
                Tab::Accounts => self.tab_accounts(ui, &state, busy, &accounts, &settings.server),
                Tab::Server => self.tab_server(ui, &state, busy, &settings.server),
                Tab::Translation => self.tab_translation(ui, &state, busy, translation.as_ref(), quests),
            });
            panel_ui.small(egui::RichText::new(format!("{} : {}", t.root, self.layout.root.display())).color(egui::Color32::from_gray(120)));
        });
    }
}
