//! FrontierFixes : compatibilité du client (DirectX 9, 2007-2019) avec un PC de 2026.
//!
//! - DXVK (option) : `Tools/dxvk/d3d9.dll` (32 bits, zlib) copié dans `Tools/mhf-iel/`. Le jeu tourne dans le
//!   processus de mhf-iel-cli.exe, qui charge `mhfo-hd.dll` depuis le dossier du client ; les dépendances de cette
//!   DLL (`d3d9.dll`) sont cherchées d'abord dans le dossier de l'exécutable, puis dans `SysWOW64`, et seulement
//!   ensuite dans le dossier courant : un `d3d9.dll` posé dans `Client/` serait ignoré. Le dossier Capcom n'est
//!   donc jamais touché. La copie n'est retirée que si c'est bien la nôtre (même empreinte).
//! - Réglages DXVK (`UserData/dxvk.conf`, transmis par `DXVK_CONFIG_FILE`) : limiteur d'images.
//! - Écran : section `[SCREEN]` de `Client/mhf.ini` (plein écran, résolutions), originale sauvegardée une fois.
//! - Mise à l'échelle Windows : couche de compatibilité `HighDpiAware` pour le seul processus du jeu.
//! - Diagnostic : D3DX9 (`d3dx9_43.dll`, absente d'un Windows neuf), polices japonaises, Vulkan pour DXVK.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::Layout;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    /// DirectX 9 traduit en Vulkan par DXVK.
    pub dxvk: bool,
    /// Images par seconde au plus (DXVK seulement ; 0 : pas de limite).
    pub max_fps: u32,
    pub fullscreen: bool,
    /// Résolution du jeu (fenêtre et plein écran) ; 0 : réglage du client inchangé.
    pub width: u32,
    pub height: u32,
    /// Ignore la mise à l'échelle de Windows (125 %, 150 %…) : image nette, taille réelle en pixels.
    pub dpi_aware: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        GameSettings { dxvk: false, max_fps: 0, fullscreen: true, width: 0, height: 0, dpi_aware: false }
    }
}

pub const RESOLUTIONS: [(u32, u32); 7] = [(1280, 720), (1366, 768), (1600, 900), (1920, 1080), (2560, 1080), (2560, 1440), (3840, 2160)];

fn dxvk_source(layout: &Layout) -> PathBuf {
    layout.root.join("Tools").join("dxvk").join("d3d9.dll")
}

fn dxvk_target(layout: &Layout) -> PathBuf {
    layout.mhf_iel_cli().with_file_name("d3d9.dll")
}

pub fn dxvk_config_file(layout: &Layout) -> PathBuf {
    layout.user_data().join("dxvk.conf")
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (crate::util::sha256_file(a), crate::util::sha256_file(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn system_dir() -> PathBuf {
    // Le jeu est un processus 32 bits : ses DLL système viennent de SysWOW64.
    let windir = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    windir.join("SysWOW64")
}

/// Une vérification du diagnostic : libellé, résultat, explication en cas d'échec.
pub struct Check {
    pub label: &'static str,
    pub ok: bool,
    pub hint: &'static str,
}

pub fn diagnostics(layout: &Layout, client_dir: &Path, game: &GameSettings) -> Vec<Check> {
    let windir = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let d3dx = [system_dir(), layout.mhf_iel_cli().with_file_name(""), client_dir.to_path_buf()].iter().any(|d| d.join("d3dx9_43.dll").is_file());
    let fonts = ["msgothic.ttc", "YuGothM.ttc", "meiryo.ttc"].iter().any(|f| windir.join("Fonts").join(f).is_file());
    let mut checks = vec![
        Check {
            label: "D3DX9 (d3dx9_43.dll)",
            ok: d3dx,
            hint: "installez « DirectX End-User Runtimes (June 2010) » de Microsoft (directx_Jun2010_redist.exe)",
        },
        Check {
            label: "Polices japonaises",
            ok: fonts,
            hint: "Paramètres > Système > Fonctionnalités facultatives > « Polices japonaises supplémentaires »",
        },
        Check { label: "Chemin du client en ASCII", ok: client_dir.to_string_lossy().is_ascii(), hint: "déplacez le dossier du jeu vers un chemin sans accent" },
    ];
    if game.dxvk {
        checks.push(Check { label: "DXVK installé (Tools/dxvk)", ok: dxvk_source(layout).is_file(), hint: "relancez Build/Get-Components.ps1" });
        checks.push(Check { label: "Vulkan 32 bits (vulkan-1.dll)", ok: system_dir().join("vulkan-1.dll").is_file(), hint: "mettez à jour le pilote de la carte graphique" });
    }
    checks
}

/// Remplace ou ajoute `key=value` dans la section `[section]` d'un .ini (octets : le fichier peut contenir du
/// Shift-JIS ailleurs ; fins de ligne conservées).
fn ini_set(ini: &mut Vec<u8>, section: &str, key: &str, value: &str) {
    let crlf = ini.windows(2).any(|w| w == b"\r\n");
    let eol: &[u8] = if crlf { b"\r\n" } else { b"\n" };
    let mut lines: Vec<Vec<u8>> = ini.split(|b| *b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l).to_vec()).collect();
    let header = format!("[{section}]").into_bytes();
    let entry = format!("{key}={value}").into_bytes();
    let prefix = format!("{key}=").into_bytes();
    match lines.iter().position(|l| l.eq_ignore_ascii_case(&header)) {
        Some(start) => {
            let end = lines[start + 1..].iter().position(|l| l.first() == Some(&b'[')).map_or(lines.len(), |n| start + 1 + n);
            match lines[start + 1..end].iter().position(|l| l.len() >= prefix.len() && l[..prefix.len()].eq_ignore_ascii_case(&prefix)) {
                Some(i) => lines[start + 1 + i] = entry,
                None => lines.insert(start + 1, entry),
            }
        }
        None => {
            if lines.last().is_some_and(|l| l.is_empty()) {
                lines.pop();
            }
            lines.push(header);
            lines.push(entry);
            lines.push(Vec::new());
        }
    }
    *ini = lines.join(eol);
}

/// Prépare le lancement : DXVK posé ou retiré, dxvk.conf, mhf.ini. Renvoie les avertissements (non bloquants).
pub fn apply(layout: &Layout, client_dir: &Path, game: &GameSettings) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    let (src, dst) = (dxvk_source(layout), dxvk_target(layout));
    if game.dxvk {
        if src.is_file() {
            if !dst.is_file() || !same_file(&src, &dst) {
                std::fs::copy(&src, &dst).map_err(|e| format!("DXVK : {e}"))?;
            }
            let mut conf = String::from("# Écrit par le launcher HomeFrontier (onglet Jeu) ; modifié à chaque lancement.\n");
            if game.max_fps > 0 {
                conf.push_str(&format!("d3d9.maxFrameRate = {}\n", game.max_fps));
            }
            std::fs::write(dxvk_config_file(layout), conf).map_err(|e| format!("dxvk.conf : {e}"))?;
        } else {
            warnings.push(format!("DXVK demandé mais absent ({}) : DirectX 9 de Windows utilisé", src.display()));
        }
    } else if dst.is_file() && src.is_file() && same_file(&src, &dst) {
        std::fs::remove_file(&dst).map_err(|e| format!("DXVK : {e}"))?;
    }

    let ini_path = client_dir.join("mhf.ini");
    if let Ok(original) = std::fs::read(&ini_path) {
        let backup = layout.user_data().join("Client-originaux").join("mhf.ini");
        if !backup.is_file() {
            std::fs::create_dir_all(backup.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
            std::fs::write(&backup, &original).map_err(|e| format!("sauvegarde de mhf.ini : {e}"))?;
        }
        let mut ini = original.clone();
        ini_set(&mut ini, "SCREEN", "FULLSCREEN_MODE", if game.fullscreen { "1" } else { "0" });
        if game.width > 0 && game.height > 0 {
            for mode in ["WINDOW", "FULLSCREEN"] {
                ini_set(&mut ini, "SCREEN", &format!("{mode}_RESOLUTION_W"), &game.width.to_string());
                ini_set(&mut ini, "SCREEN", &format!("{mode}_RESOLUTION_H"), &game.height.to_string());
            }
        }
        if ini != original {
            std::fs::write(&ini_path, ini).map_err(|e| format!("mhf.ini : {e}"))?;
        }
    } else {
        warnings.push("mhf.ini introuvable : réglages d'écran du client non appliqués".into());
    }

    for c in diagnostics(layout, client_dir, game).iter().filter(|c| !c.ok) {
        warnings.push(format!("{} : manquant — {}", c.label, c.hint));
    }
    Ok(warnings)
}

/// Variables d'environnement du processus du jeu.
pub fn environment(layout: &Layout, game: &GameSettings) -> Vec<(String, PathBuf)> {
    let mut env = Vec::new();
    if game.dxvk {
        env.push(("DXVK_CONFIG_FILE".to_string(), dxvk_config_file(layout)));
        env.push(("DXVK_LOG_PATH".to_string(), layout.logs()));
    }
    if game.dpi_aware {
        env.push(("__COMPAT_LAYER".to_string(), PathBuf::from("HighDpiAware")));
    }
    env
}

#[cfg(test)]
mod tests {
    use super::ini_set;

    #[test]
    fn ini_replace_and_add() {
        let mut ini = b"[SET]\r\nA=1\r\n[SCREEN]\r\nFULLSCREEN_MODE=1\r\nX=2\r\n[VIDEO]\r\nY=3\r\n".to_vec();
        ini_set(&mut ini, "SCREEN", "FULLSCREEN_MODE", "0");
        ini_set(&mut ini, "SCREEN", "WINDOW_RESOLUTION_W", "2560");
        ini_set(&mut ini, "OTHER", "Z", "4");
        assert_eq!(
            String::from_utf8(ini).unwrap(),
            "[SET]\r\nA=1\r\n[SCREEN]\r\nWINDOW_RESOLUTION_W=2560\r\nFULLSCREEN_MODE=0\r\nX=2\r\n[VIDEO]\r\nY=3\r\n[OTHER]\r\nZ=4\r\n"
        );
    }
}
