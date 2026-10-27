//! Traduction du client : application des chaînes de MHFrontier-Translation aux fichiers texte du jeu
//! (`dat/mhf*.bin`), avec sauvegarde des fichiers d'origine.
//!
//! `ecd.rs`, `jkr.rs`, `pointer_tables.rs`, `patch.rs` et `headers.json` viennent de mhf-outpost
//! (Mogapedia, licence MIT, `LICENSE-mhf-outpost.txt`, commit f7b6cff) ; modifications : chemins `crate::`
//! adaptés, fonction `patch::encode_for_game` ajoutée en fin de fichier, sections absentes de `headers.json`
//! ignorées et signalées par `patch::apply_translations` au lieu de tout faire échouer, et `jkr::lz_decode` qui s'arrête à la fin
//! des données comme ReFrontier (le mhfdat.bin du patch anglais annonce une taille supérieure à son contenu). Les traductions elles-mêmes (GPL-3.0) ne sont pas intégrées à l'exécutable : elles sont lues dans
//! `Localization/translations-fr.json`.
//!
//! La reconstruction ajoute les chaînes en fin de fichier : appliquer deux fois sur un fichier déjà traduit le
//! ferait grossir. On repart donc toujours des originaux sauvegardés.

#[allow(dead_code)]
mod ecd;
#[allow(dead_code)]
mod jkr;
#[allow(dead_code)]
mod patch;
#[allow(dead_code)]
mod pointer_tables;
pub mod quest;

use std::path::{Path, PathBuf};

use crate::paths::Layout;

/// Fichiers texte du client touchés par la traduction (correspondance des préfixes de `patch.rs`).
pub const TEXT_FILES: [&str; 9] = ["mhfdat.bin", "mhfpac.bin", "mhfinf.bin", "mhfjmp.bin", "mhfnav.bin", "mhfgao.bin", "mhfsqd.bin", "mhfrcc.bin", "mhfmsx.bin"];

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Fichiers d'origine (aucune sauvegarde, ou client identique à la sauvegarde).
    Original,
    /// Client modifié par rapport aux originaux sauvegardés.
    Translated,
    /// Paquet de traduction absent.
    NoPayload,
}

fn backup_dir(layout: &Layout) -> PathBuf {
    layout.user_data().join("Client-originaux").join("dat")
}

pub fn payload_file(layout: &Layout) -> PathBuf {
    layout.root.join("Localization").join("translations-fr.json")
}

/// Traductions des textes de quêtes (TSV produit par `--do quest-inventory`, colonne `fr` remplie).
pub fn quest_translations_file(layout: &Layout) -> PathBuf {
    layout.root.join("Localization").join("quetes-fr.tsv")
}

pub fn quest_backup_dir(layout: &Layout) -> PathBuf {
    layout.user_data().join("Serveur-originaux").join("quests")
}

/// Nombre de quêtes actuellement traduites (= sauvegardées), et existence du fichier de traductions.
pub fn quest_status(layout: &Layout) -> (usize, bool) {
    let translated = std::fs::read_dir(quest_backup_dir(layout)).map(|d| d.count()).unwrap_or(0);
    (translated, quest_translations_file(layout).is_file())
}

fn same_content(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => crate::util::sha256_file(a).ok() == crate::util::sha256_file(b).ok(),
        _ => false,
    }
}

pub fn status(layout: &Layout, client_dir: &Path) -> Status {
    if !payload_file(layout).is_file() {
        return Status::NoPayload;
    }
    let backups = backup_dir(layout);
    let translated = TEXT_FILES.iter().any(|f| {
        let b = backups.join(f);
        b.is_file() && !same_content(&b, &client_dir.join("dat").join(f))
    });
    if translated { Status::Translated } else { Status::Original }
}

/// Sauvegarde les originaux une seule fois (jamais écrasés ensuite : ce sont les fichiers de référence).
fn ensure_backups(layout: &Layout, client_dir: &Path) -> Result<usize, String> {
    let dir = backup_dir(layout);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut saved = 0;
    for f in TEXT_FILES {
        let (src, dst) = (client_dir.join("dat").join(f), dir.join(f));
        if src.is_file() && !dst.is_file() {
            std::fs::copy(&src, &dst).map_err(|e| format!("sauvegarde de {f} impossible : {e}"))?;
            saved += 1;
        }
    }
    Ok(saved)
}

/// Remet les fichiers d'origine dans le client. Renvoie le nombre de fichiers restaurés.
pub fn restore(layout: &Layout, client_dir: &Path) -> Result<usize, String> {
    let dir = backup_dir(layout);
    let mut restored = 0;
    for f in TEXT_FILES {
        let (src, dst) = (dir.join(f), client_dir.join("dat").join(f));
        if src.is_file() && !same_content(&src, &dst) {
            std::fs::copy(&src, &dst).map_err(|e| format!("restauration de {f} impossible : {e}"))?;
            restored += 1;
        }
    }
    Ok(restored)
}

/// Contrôle de relecture : déchiffre et décompresse chaque fichier texte du client et y cherche `needle`
/// (encodé comme le jeu, accents repliés). Renvoie, par fichier, le résultat (taille décodée, trouvé) ou l'erreur.
pub fn verify(client_dir: &Path, needle: &str) -> Vec<(String, Result<(usize, bool), String>)> {
    let folded = patch::encode_for_game(needle);
    TEXT_FILES
        .iter()
        .map(|f| {
            let result = (|| {
                let raw = std::fs::read(client_dir.join("dat").join(f)).map_err(|e| e.to_string())?;
                let le32 = |b: &[u8], o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])).unwrap_or(0);
                let (data, crc_ok) = if ecd::is_ecd(&raw) {
                    let plain = ecd::decode_ecd(&raw).map_err(|e| e.to_string())?;
                    let ok = crc32fast::hash(&plain) == le32(&raw, 12);
                    (plain, ok)
                } else {
                    (raw, true)
                };
                let data = if jkr::is_jkr(&data) {
                    let header = |o: usize, n: usize| data.get(o..o + n).map(|b| b.iter().rev().fold(0u64, |a, x| a << 8 | *x as u64)).unwrap_or(0);
                    let desc = format!(
                        "CRC ECD {} ; JKR version {} type {} début {} taille annoncée {} ; fichier déchiffré {} octets",
                        if crc_ok { "bon" } else { "FAUX" },
                        header(4, 2),
                        header(6, 2),
                        header(8, 4),
                        header(12, 4),
                        data.len()
                    );
                    jkr::decompress_jkr(&data).map_err(|e| format!("{e} [{desc}]"))?
                } else {
                    data
                };
                let found = !folded.is_empty() && data.windows(folded.len()).any(|w| w == folded.as_slice());
                Ok((data.len(), found))
            })();
            (f.to_string(), result)
        })
        .collect()
}

/// Résultat d'une application : chaînes posées par fichier, et sections du paquet que `headers.json` ne décrit pas
/// encore (ignorées).
pub struct Applied {
    pub files: Vec<(String, usize)>,
    pub skipped: Vec<String>,
}

/// Applique la traduction française en repartant des originaux.
pub fn apply(layout: &Layout, client_dir: &Path) -> Result<Applied, String> {
    let payload = payload_file(layout);
    if !payload.is_file() {
        return Err(format!("paquet de traduction absent : {}", payload.display()));
    }
    ensure_backups(layout, client_dir)?;
    restore(layout, client_dir)?;
    let (results, skipped) = patch::apply_translations(&payload, "fr", client_dir, true, true).map_err(|e| format!("{e:#}"))?;
    Ok(Applied { files: results.into_iter().map(|r| (r.file, r.count)).collect(), skipped })
}
