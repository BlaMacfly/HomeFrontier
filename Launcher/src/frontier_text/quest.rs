//! Textes des quêtes (fichiers `Server/bin/quests/*.bin` envoyés par Erupe au jeu).
//!
//! Disposition (d'après `ParseQuestBinary` d'Erupe, `quest_json_parser.go`) : le fichier est compressé en JKR ou
//! brut ; une fois décompressé, le mot de 32 bits en 0x00 pointe sur les propriétés principales `mp`, et
//! `mp + 0x28` pointe sur une table de 8 pointeurs vers des chaînes Shift-JIS terminées par un zéro, dans l'ordre :
//! titre, objectif principal, objectif A, objectif B, condition de réussite, condition d'échec, commanditaire,
//! description. Erupe accepte les quêtes non compressées (c'est ce que produit son compilateur JSON) et suit ces
//! pointeurs pour lire les textes : on traduit donc en ajoutant les chaînes françaises en fin de fichier et en
//! réécrivant les 8 pointeurs, sans toucher au reste.

use std::collections::HashMap;
use std::path::Path;

use super::jkr;

pub const FIELD_NAMES: [&str; 8] = ["titre", "objectif", "objectif_a", "objectif_b", "reussite", "echec", "commanditaire", "description"];

fn u32_at(data: &[u8], off: usize) -> Option<u32> {
    data.get(off..off + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Contenu décompressé d'un fichier de quête.
pub fn load(path: &Path) -> Result<Vec<u8>, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    if jkr::is_jkr(&raw) { jkr::decompress_jkr(&raw).map_err(|e| e.to_string()) } else { Ok(raw) }
}

/// Position de la table des 8 pointeurs de texte, si la quête en a une.
pub fn text_table(data: &[u8]) -> Option<usize> {
    let mp = u32_at(data, 0)? as usize;
    let table = u32_at(data, mp + 0x28)? as usize;
    (table != 0 && table + 32 <= data.len()).then_some(table)
}

fn read_sjis(data: &[u8], ptr: usize) -> Option<String> {
    if ptr == 0 || ptr >= data.len() {
        return None;
    }
    let end = data[ptr..].iter().position(|b| *b == 0).map(|n| ptr + n)?;
    let (s, _, _) = encoding_rs::SHIFT_JIS.decode(&data[ptr..end]);
    Some(s.into_owned())
}

/// Les 8 textes d'une quête (chaîne vide pour un emplacement vide ou illisible).
pub fn texts(data: &[u8]) -> Option<[String; 8]> {
    let table = text_table(data)?;
    let mut out: [String; 8] = Default::default();
    for (i, slot) in out.iter_mut().enumerate() {
        let ptr = u32_at(data, table + i * 4)? as usize;
        *slot = read_sjis(data, ptr).unwrap_or_default();
    }
    Some(out)
}

/// Remplace les textes d'une quête décompressée : chaque texte présent dans `translations` est ajouté en fin de
/// fichier (encodé comme le jeu, accents repliés) et son pointeur réécrit. Renvoie le nombre de textes remplacés.
pub fn retext(data: &mut Vec<u8>, translations: &HashMap<String, String>) -> usize {
    let Some(table) = text_table(data) else { return 0 };
    let originals = match texts(data) {
        Some(t) => t,
        None => return 0,
    };
    let mut replaced = 0;
    for (i, original) in originals.iter().enumerate() {
        let Some(fr) = translations.get(original).filter(|fr| !fr.is_empty() && *fr != original) else { continue };
        // Alignement sur 4 octets, comme les données d'origine.
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let ptr = data.len() as u32;
        data.extend_from_slice(&super::patch::encode_for_game(fr));
        data.push(0);
        data[table + i * 4..table + i * 4 + 4].copy_from_slice(&ptr.to_le_bytes());
        replaced += 1;
    }
    replaced
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Traductions des textes de quêtes : TSV `source<TAB>…<TAB>fr` (en-tête obligatoire ; colonnes repérées par
/// leur nom, `\n` échappés comme dans l'inventaire). Les lignes sans traduction sont ignorées.
pub fn load_translations(path: &Path) -> Result<HashMap<String, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let header: Vec<&str> = lines.next().ok_or("fichier de traduction vide")?.split('\t').collect();
    let col = |name: &str| header.iter().position(|h| h.trim() == name).ok_or(format!("colonne « {name} » absente"));
    let (src_col, fr_col) = (col("source")?, col("fr")?);
    let mut map = HashMap::new();
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let (Some(src), Some(fr)) = (cells.get(src_col), cells.get(fr_col)) else { continue };
        if !fr.trim().is_empty() {
            map.insert(unescape(src), unescape(fr));
        }
    }
    Ok(map)
}

pub struct QuestApplied {
    pub files: usize,
    pub changed: usize,
    pub texts: usize,
}

/// Applique les traductions à toutes les quêtes. On repart toujours de l'original (sauvegardé une seule fois dans
/// `backup_dir` avant la première modification d'un fichier) ; les quêtes modifiées sont écrites décompressées.
pub fn apply_dir(quest_dir: &Path, backup_dir: &Path, translations: &HashMap<String, String>) -> Result<QuestApplied, String> {
    std::fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;
    let mut result = QuestApplied { files: 0, changed: 0, texts: 0 };
    let entries = std::fs::read_dir(quest_dir).map_err(|e| format!("{}: {e}", quest_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_os_string()) else { continue };
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        result.files += 1;
        let backup = backup_dir.join(&name);
        let source = if backup.is_file() { backup.clone() } else { path.clone() };
        let Ok(mut data) = load(&source) else { continue };
        let n = retext(&mut data, translations);
        if n == 0 {
            // Plus rien à traduire dans ce fichier : on remet l'original s'il avait été modifié.
            if backup.is_file() {
                std::fs::copy(&backup, &path).map_err(|e| e.to_string())?;
            }
            continue;
        }
        if !backup.is_file() {
            std::fs::copy(&path, &backup).map_err(|e| format!("sauvegarde de {}: {e}", path.display()))?;
        }
        std::fs::write(&path, &data).map_err(|e| format!("{}: {e}", path.display()))?;
        result.changed += 1;
        result.texts += n;
    }
    Ok(result)
}

/// Remet toutes les quêtes sauvegardées. Renvoie le nombre de fichiers restaurés.
pub fn restore_dir(quest_dir: &Path, backup_dir: &Path) -> Result<usize, String> {
    let Ok(entries) = std::fs::read_dir(backup_dir) else { return Ok(0) };
    let mut n = 0;
    for entry in entries.flatten() {
        std::fs::copy(entry.path(), quest_dir.join(entry.file_name())).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

/// Inventaire des textes de toutes les quêtes : texte → (nombre d'occurrences, champs où il apparaît).
pub struct Inventory {
    pub files: usize,
    pub unreadable: Vec<String>,
    pub texts: HashMap<String, (usize, u8)>,
}

pub fn inventory(quest_dir: &Path) -> Result<Inventory, String> {
    let mut inv = Inventory { files: 0, unreadable: Vec::new(), texts: HashMap::new() };
    let entries = std::fs::read_dir(quest_dir).map_err(|e| format!("{}: {e}", quest_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        inv.files += 1;
        let parsed = load(&path).ok().and_then(|d| texts(&d));
        let Some(t) = parsed else {
            inv.unreadable.push(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            continue;
        };
        for (i, s) in t.into_iter().enumerate() {
            if s.trim().is_empty() {
                continue;
            }
            let e = inv.texts.entry(s).or_insert((0, 0));
            e.0 += 1;
            e.1 |= 1 << i;
        }
    }
    Ok(inv)
}

/// Écrit l'inventaire en TSV (texte d'origine, occurrences, champs, colonne de traduction vide), trié par
/// nombre d'occurrences décroissant. Tabulations et retours à la ligne du texte sont échappés (\t, \n).
pub fn write_inventory_tsv(inv: &Inventory, out: &Path) -> Result<(), String> {
    let escape = |s: &str| s.replace('\\', "\\\\").replace('\t', "\\t").replace('\r', "\\r").replace('\n', "\\n");
    let mut rows: Vec<_> = inv.texts.iter().collect();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
    let mut text = String::from("source\toccurrences\tchamps\tfr\n");
    for (src, (count, fields)) in rows {
        let names: Vec<&str> = (0..8).filter(|i| fields & (1 << i) != 0).map(|i| FIELD_NAMES[i]).collect();
        text.push_str(&format!("{}\t{count}\t{}\t\n", escape(src), names.join(",")));
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, text).map_err(|e| e.to_string())
}
