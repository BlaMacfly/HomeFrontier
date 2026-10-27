//! PostgreSQL portable : initialisation au premier démarrage, démarrage, base « erupe », arrêt propre.
//! Le schéma est créé par Erupe lui-même (migrations appliquées à son démarrage).

use std::time::{Duration, Instant};

use crate::credentials::Credentials;
use crate::paths::{DB_NAME, Layout};
use crate::util::{hidden, port_open};

pub fn is_initialized(layout: &Layout) -> bool {
    layout.db_data().join("PG_VERSION").is_file()
}

/// initdb : authentification scram-sha-256, UTF-8, locale C (aucune dépendance aux paramètres régionaux de Windows).
pub fn initialize(layout: &Layout, creds: &Credentials) -> Result<(), String> {
    std::fs::create_dir_all(layout.user_data()).map_err(|e| e.to_string())?;
    let pwfile = layout.user_data().join(".initdb-pw");
    std::fs::write(&pwfile, &creds.password).map_err(|e| e.to_string())?;
    let status = hidden(&layout.pgsql_bin().join("initdb.exe"))
        .arg("-D").arg(layout.db_data())
        .arg("-U").arg(&creds.user)
        .arg(format!("--pwfile={}", pwfile.display()))
        .args(["--auth=scram-sha-256", "-E", "UTF8", "--locale=C"])
        .status();
    let _ = std::fs::remove_file(&pwfile);
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("initdb a échoué (code {:?})", s.code())),
        Err(e) => Err(format!("initdb introuvable : {e}")),
    }
}

pub fn is_running(layout: &Layout) -> bool {
    hidden(&layout.pgsql_bin().join("pg_ctl.exe")).arg("status").arg("-D").arg(layout.db_data()).status().map(|s| s.success()).unwrap_or(false)
}

pub fn start(layout: &Layout, port: u16) -> Result<(), String> {
    if is_running(layout) {
        return Ok(());
    }
    if port_open(port) {
        return Err(format!("le port {port} est déjà utilisé par un autre programme (changez le port de la base dans l'onglet Serveur)"));
    }
    std::fs::create_dir_all(layout.logs()).map_err(|e| e.to_string())?;
    let status = hidden(&layout.pgsql_bin().join("pg_ctl.exe"))
        .args(["start", "-w", "-t", "60", "-D"]).arg(layout.db_data())
        .arg("-l").arg(layout.pgsql_log())
        .arg("-o").arg(format!("-p {port} -c listen_addresses=127.0.0.1"))
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("PostgreSQL n'a pas démarré (voir {})", layout.pgsql_log().display()));
    }
    // pg_ctl -w attend déjà la disponibilité ; on vérifie quand même la connexion réelle.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !is_ready(layout, port) {
        if Instant::now() > deadline {
            return Err("PostgreSQL ne répond pas".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

pub fn is_ready(layout: &Layout, port: u16) -> bool {
    hidden(&layout.pgsql_bin().join("pg_isready.exe")).args(["-h", "127.0.0.1", "-p", &port.to_string()]).status().map(|s| s.success()).unwrap_or(false)
}

/// Requête en lecture : lignes, colonnes séparées par le caractère 0x1F (aucun nom de compte ne le contient).
fn psql_rows(layout: &Layout, creds: &Credentials, port: u16, database: &str, sql: &str) -> Result<Vec<Vec<String>>, String> {
    let out = hidden(&layout.pgsql_bin().join("psql.exe"))
        .args(["-h", "127.0.0.1", "-p", &port.to_string(), "-U", &creds.user, "-d", database, "-tA", "-F", "\u{1f}", "-c", sql])
        .env("PGPASSWORD", &creds.password)
        .env("PGCLIENTENCODING", "UTF8")
        .stdout(std::process::Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("requête SQL refusée : {sql}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.is_empty()).map(|l| l.split('\u{1f}').map(str::to_string).collect()).collect())
}

/// Crée la base d'Erupe si elle n'existe pas encore.
pub fn ensure_database(layout: &Layout, creds: &Credentials, port: u16) -> Result<bool, String> {
    let exists = psql_rows(layout, creds, port, "postgres", &format!("SELECT 1 FROM pg_database WHERE datname = '{DB_NAME}'"))?;
    if !exists.is_empty() {
        return Ok(false);
    }
    psql_rows(layout, creds, port, "postgres", &format!("CREATE DATABASE {DB_NAME} ENCODING 'UTF8'"))?;
    Ok(true)
}

/// Comptes du serveur local et nombre de personnages terminés (créés en jeu) de chacun.
pub fn accounts(layout: &Layout, creds: &Credentials, port: u16) -> Result<Vec<(String, u32)>, String> {
    let rows = psql_rows(
        layout,
        creds,
        port,
        DB_NAME,
        "SELECT u.username, COUNT(c.id) FROM users u \
         LEFT JOIN characters c ON c.user_id = u.id AND COALESCE(c.name, '') <> '' \
         GROUP BY u.id, u.username ORDER BY u.id",
    )?;
    Ok(rows.into_iter().filter_map(|r| Some((r.first()?.clone(), r.get(1)?.parse().ok()?))).collect())
}

pub fn stop(layout: &Layout) -> Result<(), String> {
    if !is_running(layout) {
        return Ok(());
    }
    let status = hidden(&layout.pgsql_bin().join("pg_ctl.exe"))
        .args(["stop", "-w", "-t", "60", "-m", "fast", "-D"]).arg(layout.db_data())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("PostgreSQL ne s'est pas arrêté".into()) }
}
