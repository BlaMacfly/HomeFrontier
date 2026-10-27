//! Petits utilitaires : processus sans fenêtre, ports, mots de passe, horodatage.

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

/// Commande sans fenêtre de console. Les sorties ne sont jamais des tubes laissés ouverts : pg_ctl transmet ses
/// descripteurs au serveur PostgreSQL qu'il démarre, et un tube hérité ne se fermerait qu'à l'arrêt de la base.
pub fn hidden(program: &std::path::Path) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    c
}

/// Processus serveur : console cachée et groupe de processus propre, pour pouvoir lui envoyer Ctrl+Pause.
pub fn hidden_group(program: &std::path::Path) -> Command {
    let mut c = hidden(program);
    c.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    c
}

/// Un service écoute-t-il déjà sur ce port local ?
pub fn port_open(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Mot de passe aléatoire alphanumérique (compte SQL local).
pub fn random_password(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("générateur aléatoire du système");
    bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

/// Empreinte SHA-256 d'un fichier, en hexadécimal.
pub fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Horodatage local « AAAA-MM-JJ HH:MM:SS » pour les journaux.
pub fn timestamp() -> String {
    // SAFETY : GetLocalTime remplit la structure passée, sans autre effet.
    let t = unsafe {
        let mut t: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}
