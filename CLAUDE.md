# HomeFrontier — instructions de session

Monster Hunter Frontier ZZ en solo, hors ligne, **intégralement en français** (client et serveur), sur le
modèle de HomeCraft Edition Vanilla (`../HomeCraft`, même auteur : on peut en reprendre le code et les
conventions). Projet distinct des serveurs Sylvania. Répondre en français.

## Lire d'abord
1. `Documentation/01-Audit.md` : composants retenus, problème d'encodage du français, plan par phases.

## Architecture
- Serveur : **Erupe** (Go, MIT, dépôt `Mezeporta/Erupe` archivé le 2026-07-18, version figée v9.4.1).
  Ports locaux : PostgreSQL 5433, sign 53312, entrance 53310, channel 54001+, API HTTP 8080, tous sur 127.0.0.1.
- Base : PostgreSQL portable (binaires zip EDB), données dans `UserData/Database`.
- Lancement du client : **mhf-iel** (Rust nightly, cible `i686-pc-windows-msvc`, charge `mhfo-hd.dll`) appelé
  en sous-processus (`Tools/mhf-iel/mhf-iel-cli.exe`) par le launcher 64 bits ; jeton et personnages obtenus
  par l'API Erupe (`/v2/login`, `/v2/register`, `/v2/characters`).
- Launcher : `Launcher/` en Rust + egui, comme HomeCraft.

## Dossier de test (racine ASCII)
Le dépôt (ce dossier) contient les sources ; l'installation de test est `C:\Users\Marty\HomeFrontier`
(Client/ extrait de l'archive « Full Game + Rain patch installed + English patch », Server/erupe, Server/pgsql,
UserData/, Logs/, et le launcher copié en `HomeFrontier.exe`).
- Composants : `Build/Get-Components.ps1 -Root <racine>` (ou les mêmes étapes à la main : empreintes du manifeste,
  `tar.exe` de Windows, pas celui de Git Bash).
- Test du cycle complet : `"HomeFrontier.exe" --start --do acccreate <compte> <mdp> --quit-after 45`,
  puis lire `Logs/launcher.log` et vérifier qu'aucun `postgres.exe` / `erupe-ce.exe` ne reste.

- Boot stub : clone de `rockisch/mhf-iel` au commit du manifeste dans `Build/mhf-iel-src`, compilé par
  `cargo +nightly build --release --locked -p mhf-iel-cli` (cible i686 imposée par son `.cargo/config.toml`),
  puis `target/i686-pc-windows-msvc/release/mhf-iel-cli.exe` copié dans `<racine>/Tools/mhf-iel/`.
- Test du lancement : `--start --do play <compte> <mdp> [n° de personnage]` (sans n° : nouveau personnage).
  La configuration passée au jeu (jeton compris) est écrite dans `UserData/mhf-iel-config.json`, la sortie du
  boot stub dans `Logs/mhf-iel.log`. Réglages repris de mhf-outpost : `server_port` 53310 (entrance), et pour un
  nouveau personnage `POST /v2/characters` (jeton Bearer) puis `char_new: true`.
- Réglages (`UserData/settings.json`, `settings.rs`) : mode **Local** (le launcher démarre PostgreSQL + Erupe ;
  `host` = adresse annoncée au jeu, 127.0.0.1 en solo, IP du PC pour le réseau local) ou **Distant** (rien n'est
  démarré, connexion à un Erupe existant). Ports API, sign, entrance, 2 canaux consécutifs, base : réglables
  dans l'onglet Serveur, serveur arrêté uniquement. Limite : mhf-iel fixe en dur `server_host:8080` comme
  adresse secondaire du jeu (`alt_ip_address`), quel que soit le port API choisi.
- Comptes : onglet Comptes (liste lue dans la base en local, `users` + personnages nommés ; en distant, comptes
  déjà utilisés mémorisés dans les réglages). Aucun mot de passe enregistré.
- Interface : `--tab jouer|comptes|serveur` pour ouvrir un onglet (captures de test). Titre en Cinzel
  (`assets/fonts/Cinzel.ttf`, SIL OFL 1.1, licence jointe `OFL-Cinzel.txt`, source google/fonts).
- Traduction du client (onglet Français, `frontier_text/`) : code ECD/JKR/tables de pointeurs repris de
  mhf-outpost (MIT, modifications listées en tête de `frontier_text/mod.rs`), paquet
  `Localization/translations-fr.json` (MHFrontier-Translation v0.3.0, GPL-3.0, jamais intégré à l'exe). Les
  9 fichiers `dat/mhf*.bin` d'origine sont sauvegardés une fois dans `UserData/Client-originaux/dat/` ; chaque
  application repart de ces originaux (la reconstruction ajoute en fin de fichier : ne jamais appliquer sur un
  fichier déjà traduit). Commandes de test : `--do translate`, `--do untranslate`,
  `--do translate-verify "<texte>"` (décode les 9 fichiers et y cherche le texte, accents repliés).
  Particularités de notre client (patch anglais) : `mhfdat.bin` est en JKR type 3 et son flux LZ s'arrête
  10 octets avant la taille annoncée (toléré jusqu'à 64 octets, `jkr::LZ_MAX_TRUNCATED_TAIL`) ; 4 sections de
  menus du paquet v0.3.0 (46 chaînes) manquent dans `headers.json` et sont ignorées.
- Quêtes du serveur (`frontier_text/quest.rs`) : 54 977 fichiers (9 170 quêtes × variantes jour/nuit et saison),
  JKR type 3, textes en japonais (8 par quête via la table pointée par `mp + 0x28`). `--do quest-inventory <tsv>`
  produit l'inventaire des textes uniques (14 223) ; `Localization/quetes-fr.tsv` (colonnes `source`, `fr`,
  `\n` échappés) est la traduction versionnée dans le dépôt, copiée dans `<racine>/Localization/` ;
  `--do quest-translate` / `--do quest-untranslate`. Application : on repart de l'original (sauvegardé une fois
  dans `UserData/Serveur-originaux/quests/`), chaînes françaises ajoutées en fin de fichier, 8 pointeurs
  réécrits, fichier écrit décompressé (Erupe et le jeu l'acceptent : c'est ce que produit le compilateur JSON
  d'Erupe). Environ 3 min 20 pour tout ; redémarrer le serveur ensuite (cache de quêtes d'Erupe).
  Vocabulaire : 狩猟 « Chasser », 討伐 « Tuer », 捕獲 « Capturer », 亜種 nom français de sous-espèce,
  変種 « variante », 報酬金ゼロ « Prime à zéro ».
- Traduction automatique des quêtes **hors ligne, sans Claude** (demande utilisateur du 2026-10-28) :
  `Tools/traduire-quetes/` (Python 3.12 installé par winget pour l'utilisateur, `.venv` privé, CTranslate2 +
  NLLB-200 distilled 600M converti en int8, CC-BY-NC). `Installer.cmd`, puis `Traduire.cmd -Racine <racine>` →
  `<racine>/Localization/quetes-fr-auto.tsv`, appliqué par le launcher **après** `quetes-fr.tsv` (manuel
  prioritaire). Glossaire `glossaire.tsv` (noms, surnoms, armes), catégories de titres `etiquettes.tsv`, règles
  d'objectifs et de titres, garde-fou contre les phrases inventées par NLLB sur les textes courts (repli sur le
  japonais), recoupage à 26 caractères, rapport
  `quetes-fr-auto-rapport.tsv`. L'inventaire lit les originaux sauvegardés (jamais les quêtes traduites).
  Le script empêche la veille de Windows pendant qu'il tourne (`SetThreadExecutionState`).
- Scénarios du serveur (`frontier_text/scenario.rs`, `Server/bin/scenarios`, 145 376 fichiers) : conteneur
  (2 tailles u32 BE, chunk0, chunk1, puis u32 + chunk2), format dans `Build/erupe-src/docs/scenario-format.md`.
  Traduits : chunks à sous-en-tête (2 835 textes uniques, 1,2 million d'occurrences). Métadonnées recalculées
  (vérifié sur tous les fichiers) : chunk0 (20 o) u16 n° 5 = longueur de la chaîne 0 ; chunk1 (44 o, toujours
  10 chaînes) u16 n° 8 à 17 = départs des chaînes, n° 21 = taille des chaînes + constante ; taille totale u16 en +2.
  Erupe, lui, recopie les métadonnées sans les recalculer (ne pas passer par ses .json pour traduire).
  Non traduits : chunks JKR (type 3, 5 635 textes, 79 000 chunks) — chunk2 = enregistrements séquentiels
  (en-tête 17 o + 2 chaînes, sans pointeurs, à confirmer), chunk1 JKR = bytecode de dialogue (sauts possibles :
  rétro-ingénierie nécessaire). Commandes `--do scenario-inventory <tsv>`, `scenario-translate`,
  `scenario-untranslate` ; originaux dans `UserData/Serveur-originaux/scenarios/` ; traductions
  `Localization/scenarios-fr.tsv` (manuel) puis `scenarios-fr-auto.tsv` ; `Traduire.cmd -Textes scenarios`.
  Codes : `@RETURN` (page), `~Cnn` (couleur, `~C00` fin), `@MYNAME`, `@MYCLEARNUM`, `@MYMONEY`, `@CHAHR`.
- Fond d'écran : `Launcher/assets/fond.jpg` n'est **pas** versionné (logo Capcom, décision utilisateur du
  2026-10-28) ; `build.rs` l'intègre s'il est présent (`cfg(has_fond)`), sinon fond sombre. Après l'avoir
  remis en place, `touch` le fichier pour que cargo recompile.
- Dépôt : https://github.com/BlaMacfly/HomeFrontier (public), licence propriétaire `LICENCE.txt` adaptée de
  HomeCraft (article 1.1 : coordonnées du titulaire à compléter).
- Langue de l'interface : `i18n.rs` (Français / English / 日本語, réglage `ui_language`, sélecteur dans
  l'en-tête). Le japonais utilise une police de Windows (Yu Gothic, Meiryo ou MS Gothic) ; le journal reste
  en français. Les réglages et identifiants tolèrent un BOM UTF-8 ; un `db-credentials.json` illisible n'est
  jamais écrasé.
- Visuels : `Launcher/assets/fond.jpg` (affiche fournie par l'utilisateur, contient le logo Capcom : à
  remplacer avant toute diffusion), `logo-source.png` → `logo-512.png` et `frontier.ico` (icône de l'exe).

- Compatibilité 2026 (« FrontierFixes », `fixes.rs`, onglet **Jeu**, `--tab jeu`) : réglages `settings.game`
  (plein écran, résolution, DPI, DXVK, limite d'images) appliqués à chaque lancement. DXVK 3.1.1 (manifeste,
  `Get-Components.ps1` → `<racine>/Tools/dxvk/d3d9.dll` + licence zlib `Build/licences/LICENSE-DXVK.txt`) est copié
  dans `Tools/mhf-iel/` : le jeu tourne dans le processus de mhf-iel-cli, et un `d3d9.dll` dans `Client/` serait
  ignoré (dossier de l'exe puis SysWOW64 avant le dossier courant). `mhf.ini` [SCREEN] modifié, original sauvegardé
  dans `UserData/Client-originaux/mhf.ini`. Diagnostic : `d3dx9_43.dll` (non redistribuée : renvoi vers
  l'installateur DirectX June 2010), polices japonaises, Vulkan 32 bits, chemin ASCII. Rien de tout cela n'est
  encore testé en jeu (sens de `FULLSCREEN_MODE` à confirmer).
- Tests du launcher : `cargo test --release` (31 tests ; `hex` en dépendance de test pour ceux repris de
  mhf-outpost).

## Pièges connus
- Mondes : le client filtre les rangs selon le `Type` de l'entrée. Un nouveau personnage est refusé
  (« Insufficient Hunter Rank ») sur un monde Normal (type 1) : garder un monde Newbie (type 3) en premier.
- `/v2/login` ne liste pas un emplacement réservé par `POST /v2/characters` dont la création n'a pas été
  terminée en jeu ; Erupe réattribue ce même emplacement au prochain `POST /v2/characters`.
- `pg_ctl start` transmet ses descripteurs à `postgres.exe` : ne jamais lire sa sortie par un tube (le tube ne
  se ferme qu'à l'arrêt de la base). Sorties vers null ou fichier (`util::hidden`).
- Erupe lit `config.json` dans son dossier courant (`Server/Config`) et lance un assistant web sur le port 8080
  s'il ne le trouve pas : toujours générer la config avant le démarrage.
- Arrêt propre d'Erupe : Ctrl+Pause envoyé à sa console cachée (`server::stop`), jamais `taskkill /F` sauf
  en dernier recours (les personnages sont sauvegardés à la déconnexion).

## Règles
- `Client/` (client MHF de l'utilisateur) et `Server/bin/` (quêtes, scénarios, événements : données Capcom) :
  **jamais redistribués, jamais dans une archive ni un dépôt**. Le livrable ne contient aucun fichier Capcom.
- `UserData/` contient la base (personnages) : ne jamais supprimer ni réimporter sans sauvegarde et accord.
- Composants externes : uniquement via `Build/manifest.json` (URL, commit, SHA-256 figés).
- Scripts PowerShell : UTF-8 avec BOM, pas de chemins absolus codés en dur.
- Le client doit tourner dans un chemin ASCII : le dossier racine actuel contient « è » (« frontière »), qui
  n'existe pas en CP932. À vérifier sur le client avant de s'appuyer sur `Client/` sous cette racine.
- Texte envoyé au client : Shift-JIS. Ne jamais supposer qu'un caractère accentué passe : voir l'audit.
