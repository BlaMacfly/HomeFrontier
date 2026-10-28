//! Textes des scénarios (fichiers `Server/bin/scenarios/*.bin` envoyés par Erupe au jeu : histoires, dialogues des
//! PNJ, listes d'épisodes).
//!
//! Disposition (d'après `docs/scenario-format.md` d'Erupe, vérifiée sur les 145 376 fichiers du serveur) : deux
//! tailles u32 gros-boutistes (chunk0, chunk1), les deux chunks, puis éventuellement u32 + chunk2. Un chunk dont
//! le 2e octet vaut 0 a un sous-en-tête de 8 octets (type, 0, taille totale u16 LE, nombre d'entrées, ?, taille des
//! métadonnées, ?), puis les métadonnées, des chaînes Shift-JIS terminées par un zéro, un octet 0xFF et
//! d'éventuelles données de dialogue. Les métadonnées contiennent des positions de chaînes, recalculées ici :
//!   - chunk0 (métadonnées de 20 octets) : u16 n° 5 = longueur de la chaîne 0, zéro compris ;
//!   - chunk1 (44 octets, toujours 10 chaînes) : u16 n° 8 à 17 = positions de départ des 10 chaînes (dans un ordre
//!     fixe), u16 n° 21 = taille des chaînes + constante.
//! Les données placées après le 0xFF sont adressées depuis leur propre début : elles ne bougent pas. Les chunks
//! compressés en JKR (scripts de dialogue, menus) ne sont pas encore traduits : leur structure interne (bytecode)
//! n'est pas documentée.

use std::collections::HashMap;
use std::path::Path;

/// Taille maximale d'un chunk acceptée par le client (`FUN_11525c60` de `mhfo-hd.dll`, d'après Erupe).
const CHUNK_LIMIT: usize = 0x8000;

pub const FIELD_NAMES: [&str; 3] = ["scenario0", "scenario1", "scenario_dialogue"];

struct Container {
    chunks: [Vec<u8>; 3],
    has_chunk2: bool,
}

fn parse(data: &[u8]) -> Option<Container> {
    let be = |o: usize| data.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let (c0, c1) = (be(0)?, be(4)?);
    let chunk0 = data.get(8..8 + c0)?.to_vec();
    let chunk1 = data.get(8 + c0..8 + c0 + c1)?.to_vec();
    let rest = 8 + c0 + c1;
    let (chunk2, has_chunk2) = match be(rest) {
        Some(c2) => (data.get(rest + 4..rest + 4 + c2)?.to_vec(), true),
        None => (Vec::new(), false),
    };
    Some(Container { chunks: [chunk0, chunk1, chunk2], has_chunk2 })
}

fn build(c: &Container) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + c.chunks.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&(c.chunks[0].len() as u32).to_be_bytes());
    out.extend_from_slice(&(c.chunks[1].len() as u32).to_be_bytes());
    out.extend_from_slice(&c.chunks[0]);
    out.extend_from_slice(&c.chunks[1]);
    if c.has_chunk2 {
        out.extend_from_slice(&(c.chunks[2].len() as u32).to_be_bytes());
        out.extend_from_slice(&c.chunks[2]);
    }
    out
}

/// Chunk à sous-en-tête découpé : en-tête + métadonnées, chaînes (sans leur zéro), et ce qui suit (0xFF compris).
struct Subheader<'a> {
    head: &'a [u8],
    strings: Vec<&'a [u8]>,
    tail: &'a [u8],
}

fn split_subheader(chunk: &[u8]) -> Option<Subheader<'_>> {
    if chunk.len() < 8 || chunk.starts_with(b"JKR\x1a") || chunk[1] != 0 {
        return None;
    }
    let start = 8 + chunk[6] as usize;
    let mut pos = start;
    let mut strings = Vec::new();
    while pos < chunk.len() && chunk[pos] != 0xFF {
        let end = pos + chunk[pos..].iter().position(|b| *b == 0)?;
        strings.push(&chunk[pos..end]);
        pos = end + 1;
    }
    Some(Subheader { head: chunk.get(..start)?, strings, tail: &chunk[pos..] })
}

fn decode(s: &[u8]) -> String {
    encoding_rs::SHIFT_JIS.decode(s).0.into_owned()
}

// ── Chunks compressés (JKR type 3) ───────────────────────────────────────────────────────────────────────────
//
// Une fois décompressés : chunk0 = liste « {u8 index}{chaîne} » ; chunk2 = enregistrements (17 octets + 2 chaînes) ;
// chunk1 = script de dialogue (bytecode) avec les répliques en ligne, terminées par un zéro, sans champ de longueur
// ni pointeur vers elles (vérifié statistiquement sur les 135 scripts du serveur). Dans le bytecode, seules les
// chaînes précédées d'un octet nul sont traduites ; les choix de menu (précédés d'octets de contrôle) restent en
// japonais. Une chaîne doit contenir au moins 2 caractères japonais, pour ne jamais prendre un opérande pour du texte.

fn sjis_run(raw: &[u8], start: usize) -> Option<usize> {
    let mut j = start;
    while j < raw.len() {
        match raw[j] {
            0 => return (j > start).then_some(j),
            0x20..=0x7e | 0x0d | 0x0a | 0xa1..=0xdf => j += 1,
            0x81..=0x9f | 0xe0..=0xfc if matches!(raw.get(j + 1), Some(0x40..=0x7e | 0x80..=0xfc)) => j += 2,
            _ => return None,
        }
    }
    None
}

fn japanese_chars(s: &str) -> usize {
    s.chars().filter(|c| matches!(*c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff)).count()
}

/// Chaînes d'un blob décompressé : (début, fin = position du zéro).
fn blob_strings(index: usize, raw: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    if index == 0 {
        while i < raw.len() {
            if raw[i] == 0 {
                i += 1;
                continue;
            }
            let Some(end) = sjis_run(raw, i + 1) else { return Vec::new() };
            if japanese_chars(&decode(&raw[i + 1..end])) >= 2 {
                spans.push((i + 1, end));
            }
            i = end + 1;
        }
        return spans;
    }
    while i < raw.len() {
        if raw[i] != 0 && (i == 0 || raw[i - 1] == 0) {
            if let Some(end) = sjis_run(raw, i) {
                if japanese_chars(&decode(&raw[i..end])) >= 2 {
                    spans.push((i, end));
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    spans
}

fn decompress_blob(chunk: &[u8]) -> Option<Vec<u8>> {
    let comp_type = u16::from_le_bytes([*chunk.get(6)?, *chunk.get(7)?]);
    (super::jkr::is_jkr(chunk) && comp_type == 3).then(|| super::jkr::decompress_jkr(chunk).ok()).flatten()
}

fn retext_blob(index: usize, chunk: &[u8], translations: &HashMap<String, String>) -> Option<(Vec<u8>, usize)> {
    let raw = decompress_blob(chunk)?;
    let mut out = Vec::with_capacity(raw.len() * 2);
    let mut last = 0;
    let mut replaced = 0;
    for (start, end) in blob_strings(index, &raw) {
        let original = decode(&raw[start..end]);
        let Some(fr) = translations.get(&original).filter(|fr| !fr.is_empty() && **fr != original) else { continue };
        out.extend_from_slice(&raw[last..start]);
        out.extend_from_slice(&super::patch::encode_for_game(fr));
        last = end;
        replaced += 1;
    }
    if replaced == 0 {
        return None;
    }
    out.extend_from_slice(&raw[last..]);
    let version = u16::from_le_bytes([chunk[4], chunk[5]]);
    let packed = super::jkr::compress_jkr_lz(&out, version);
    // Le client ignore un chunk de plus de 32 Ko ; et on relit ce qu'on vient de compresser.
    if packed.len() > CHUNK_LIMIT || super::jkr::decompress_jkr(&packed).ok()? != out {
        return None;
    }
    Some((packed, replaced))
}

/// Textes d'un fichier : (champ, texte) ; champ 0/1 = chunk à sous-en-tête, 2 = chunk compressé.
fn texts(data: &[u8], cache: &mut HashMap<Vec<u8>, Vec<String>>) -> Option<Vec<(usize, String)>> {
    let c = parse(data)?;
    let mut out = Vec::new();
    for (i, chunk) in c.chunks.iter().enumerate() {
        if super::jkr::is_jkr(chunk) {
            let strings = cache.entry(chunk.clone()).or_insert_with(|| {
                decompress_blob(chunk).map(|raw| blob_strings(i, &raw).iter().map(|(s, e)| decode(&raw[*s..*e])).collect()).unwrap_or_default()
            });
            out.extend(strings.iter().map(|s| (2, s.clone())));
        } else if i < 2 {
            if let Some(sh) = split_subheader(chunk) {
                out.extend(sh.strings.iter().map(|s| (i, decode(s))));
            }
        }
    }
    Some(out)
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn set_u16(b: &mut [u8], o: usize, v: usize) -> Option<()> {
    let v = u16::try_from(v).ok()?;
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    Some(())
}

/// Reconstruit un chunk à sous-en-tête avec ses chaînes traduites. `None` si rien n'est traduit ou si le chunk
/// n'a pas une disposition connue (il est alors laissé tel quel).
fn retext_chunk(index: usize, chunk: &[u8], translations: &HashMap<String, String>) -> Option<(Vec<u8>, usize)> {
    let sh = split_subheader(chunk)?;
    let meta_size = chunk[6] as usize;
    let known = match index {
        0 => meta_size == 20 && !sh.strings.is_empty() && u16_at(chunk, 8 + 10) as usize == sh.strings[0].len() + 1,
        _ => meta_size == 44 && sh.strings.len() == 10,
    };
    if !known {
        return None;
    }
    let mut replaced = 0;
    let new_strings: Vec<Vec<u8>> = sh
        .strings
        .iter()
        .map(|s| {
            let original = decode(s);
            (s, translations.get(&original).filter(|fr| !fr.is_empty() && **fr != original))
        })
        .map(|(s, fr)| match fr {
            Some(fr) => {
                replaced += 1;
                super::patch::encode_for_game(fr)
            }
            None => s.to_vec(),
        })
        .collect();
    if replaced == 0 {
        return None;
    }
    let cumulative = |lens: &mut dyn Iterator<Item = usize>| {
        let mut acc = vec![0usize];
        for l in lens {
            acc.push(acc.last().unwrap() + l + 1);
        }
        acc
    };
    let old_cum = cumulative(&mut sh.strings.iter().map(|s| s.len()));
    let new_cum = cumulative(&mut new_strings.iter().map(Vec::len));
    let delta = *new_cum.last()? as isize - *old_cum.last()? as isize;

    let mut out = sh.head.to_vec();
    for s in &new_strings {
        out.extend_from_slice(s);
        out.push(0);
    }
    out.extend_from_slice(sh.tail);
    if out.len() > CHUNK_LIMIT {
        return None;
    }
    let total = u16_at(chunk, 2) as isize + delta;
    set_u16(&mut out, 2, usize::try_from(total).ok()?)?;
    if index == 0 {
        set_u16(&mut out, 8 + 10, new_strings[0].len() + 1)?;
    } else {
        for field in 8..=17 {
            let old = u16_at(chunk, 8 + field * 2) as usize;
            let k = old_cum.iter().position(|c| *c == old)?;
            set_u16(&mut out, 8 + field * 2, new_cum[k])?;
        }
        let size = u16_at(chunk, 8 + 21 * 2) as isize + delta;
        set_u16(&mut out, 8 + 21 * 2, usize::try_from(size).ok()?)?;
    }
    Some((out, replaced))
}

/// Chunks compressés déjà retraduits (beaucoup de fichiers partagent le même blob : 2 listes pour 61 389 chunk2).
type BlobCache = HashMap<(usize, Vec<u8>), Option<(Vec<u8>, usize)>>;

/// Remplace les textes d'un fichier de scénario. Renvoie le fichier reconstruit et le nombre de textes remplacés.
fn retext(data: &[u8], translations: &HashMap<String, String>, blobs: &mut BlobCache) -> Option<(Vec<u8>, usize)> {
    let mut c = parse(data)?;
    let mut replaced = 0;
    for i in 0..3 {
        let new = if super::jkr::is_jkr(&c.chunks[i]) {
            blobs.entry((i, c.chunks[i].clone())).or_insert_with(|| retext_blob(i, &c.chunks[i], translations)).clone()
        } else if i < 2 {
            retext_chunk(i, &c.chunks[i], translations)
        } else {
            None
        };
        if let Some((chunk, n)) = new {
            c.chunks[i] = chunk;
            replaced += n;
        }
    }
    (replaced > 0).then(|| (build(&c), replaced))
}

pub struct ScenarioApplied {
    pub files: usize,
    pub changed: usize,
    pub texts: usize,
}

/// Applique les traductions à tous les scénarios, en repartant des originaux (sauvegardés une fois dans
/// `backup_dir`, comme pour les quêtes).
pub fn apply_dir(dir: &Path, backup_dir: &Path, translations: &HashMap<String, String>) -> Result<ScenarioApplied, String> {
    std::fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;
    let mut result = ScenarioApplied { files: 0, changed: 0, texts: 0 };
    let mut blobs = BlobCache::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        result.files += 1;
        let backup = backup_dir.join(entry.file_name());
        let source = if backup.is_file() { &backup } else { &path };
        let Ok(data) = std::fs::read(source) else { continue };
        match retext(&data, translations, &mut blobs) {
            Some((out, n)) => {
                if !backup.is_file() {
                    std::fs::copy(&path, &backup).map_err(|e| format!("sauvegarde de {}: {e}", path.display()))?;
                }
                std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
                result.changed += 1;
                result.texts += n;
            }
            None if backup.is_file() => {
                std::fs::copy(&backup, &path).map_err(|e| e.to_string())?;
            }
            None => {}
        }
    }
    Ok(result)
}

/// Remet tous les scénarios sauvegardés. Renvoie le nombre de fichiers restaurés.
pub fn restore_dir(dir: &Path, backup_dir: &Path) -> Result<usize, String> {
    super::quest::restore_dir(dir, backup_dir)
}

/// Inventaire des textes (originaux sauvegardés lus en priorité), même structure que celui des quêtes.
pub fn inventory(dir: &Path, backup_dir: &Path) -> Result<super::quest::Inventory, String> {
    let mut inv = super::quest::Inventory { files: 0, unreadable: Vec::new(), texts: HashMap::new() };
    let mut cache = HashMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let mut path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let backup = backup_dir.join(entry.file_name());
        if backup.is_file() {
            path = backup;
        }
        inv.files += 1;
        let Some(t) = std::fs::read(&path).ok().and_then(|d| texts(&d, &mut cache)) else {
            inv.unreadable.push(entry.file_name().to_string_lossy().into_owned());
            continue;
        };
        for (chunk, s) in t {
            if s.trim().is_empty() {
                continue;
            }
            let e = inv.texts.entry(s).or_insert((0, 0));
            e.0 += 1;
            e.1 |= 1 << chunk;
        }
    }
    Ok(inv)
}
