//! Ressources Windows de l'exécutable : icône (Explorateur, barre des tâches, raccourcis) et informations de
//! version. L'icône est générée depuis assets\logo-source.png (logo fourni par l'utilisateur).
//!
//! L'image de fond `assets/fond.jpg` n'est pas dans le dépôt (elle reprend le logo de Capcom) : elle est intégrée
//! si elle est présente (`cfg(has_fond)`), sinon le launcher affiche un fond sombre.

fn main() {
    println!("cargo:rerun-if-changed=assets/frontier.ico");
    println!("cargo:rerun-if-changed=assets/fond.jpg");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(has_fond)");
    if std::path::Path::new("assets/fond.jpg").is_file() {
        println!("cargo:rustc-cfg=has_fond");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/frontier.ico");
        res.set("ProductName", "HomeFrontier");
        res.set("FileDescription", "HomeFrontier");
        res.set("CompanyName", "BlaMacfly");
        res.set("LegalCopyright", "© 2026 BlaMacfly. Tous droits réservés.");
        res.set("OriginalFilename", "HomeFrontier.exe");
        res.set_language(0x040c); // français (France)
        res.compile().expect("ressources Windows (winresource : rc.exe du SDK Windows requis)");
    }
}
