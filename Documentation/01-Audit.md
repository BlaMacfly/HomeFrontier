# 01 — Audit : HomeFrontier

*Rédigé le 2026-10-27, avant réception du client. Les points marqués « à vérifier » seront tranchés sur le vrai client.*

## Objectif

Monster Hunter Frontier ZZ (MHF-Z, fermé par Capcom le 18/12/2019) en **solo, hors ligne, intégralement en
français, côté client comme côté serveur**, avec un launcher à un bouton sur le modèle de HomeCraft Edition
Vanilla. Le livrable ne contient **aucun fichier Capcom** : l'utilisateur fournit son client et les fichiers de
quêtes.

## Composants retenus

| Rôle | Composant | État vérifié le 2026-10-27 |
|---|---|---|
| Serveur de jeu | [Mezeporta/Erupe](https://github.com/Mezeporta/Erupe) v9.4.1 (Go, MIT) — ex-ZeruLight | dépôt **archivé le 2026-07-18** ; binaire Windows dans la release, SHA-256 figé dans `Build/manifest.json` |
| Base de données | PostgreSQL portable (zip « binaries » EDB) | Erupe applique lui-même ses migrations au démarrage ; version à figer |
| Données serveur | `bin/quests`, `bin/scenarios`, `bin/events`, `rengoku_data.bin` | données Capcom, fournies par l'utilisateur ; sans elles le client plante |
| Lancement du client | [rockisch/mhf-iel](https://github.com/rockisch/mhf-iel) (Rust, MIT) ou le « boot stub » de [Mogapedia/mhf-outpost](https://github.com/Mogapedia/mhf-outpost) (Rust, MIT) | charge `mhfo-hd.dll` sans `mhf.exe`, `mhl.dll`, Internet Explorer ni GameGuard ; **32 bits obligatoire** |
| Traduction du client | [Mogapedia/MHFrontier-Translation](https://github.com/Mogapedia/MHFrontier-Translation) v0.3.0 (GPL-3.0) | **63 023 chaînes françaises** (CSV `index,source,target`), actif (2026-09-28) |
| Outil de texte | FrontierTextHandler ≥ 1.9.0 (Python, Houmgaor), ou le code Rust de mhf-outpost qui gère ECD/JKR | dépôt FrontierTextHandler en 404 au moment de l'audit : préférer le code de mhf-outpost |
| Référence anglaise | [Blizz127/…-English-Translation](https://github.com/Blizz127/Monster-Hunter-Frontier-PC-English-Translation) (GPL-3.0) | contient aussi des patchs `quests`/`scenarios` pour Erupe : base de travail pour le français serveur |

### Pourquoi ne pas simplement utiliser mhf-outpost ?

mhf-outpost (Tauri + Vue) est un launcher **client** pour serveurs Erupe distants : connexion, synchronisation
des fichiers depuis un serveur de patchs, application de la traduction FR, lancement sans IE. Il ne démarre
**aucun serveur local**. HomeFrontier ajoute ce qui manque pour le solo : PostgreSQL et Erupe portables
pilotés par le launcher, comptes locaux, sauvegardes, réparation, réglages du serveur, traduction serveur. On
réutilise son code MIT (boot stub, ECD/JKR, application des CSV) plutôt que de le réécrire, en le créditant.

## Architecture

```
HomeFrontier.exe   launcher Rust 64 bits (egui), comme HomeCraft
├─ db.rs        PostgreSQL portable : initdb au 1er démarrage, pg_ctl start/stop, 127.0.0.1:5433
├─ server.rs    génération de Server/Config/config.json, erupe.exe (sign 53312, entrance 53310, channel 54001, API 8080)
├─ account.rs   API Erupe : /v2/register, /v2/login (jeton + personnages), /v2/characters
├─ client.rs    contrôle du client, application de la traduction, lancement du boot stub 32 bits
└─ i18n.rs      chaînes du launcher
Tools/mhf-iel/  boot stub 32 bits (mhf-iel-cli.exe) : reçoit jeton, personnage, hôte et port, charge mhfo-hd.dll
Server/         erupe/, pgsql/, bin/ (données utilisateur), Config/ (généré)
Localization/   traduction FR serveur (quêtes/scénarios JSON) et compléments client
UserData/       base vivante, réglages, sauvegardes
```

Tout écoute sur 127.0.0.1 uniquement. Le launcher 64 bits ne peut pas charger `mhfo-hd.dll` (32 bits) : le
lancement passe forcément par un sous-processus 32 bits.

## Le problème central : les accents

**Confirmé :**
- Le client affiche son texte en **CP932** (Shift-JIS Windows) avec une **police bitmap propre au jeu** qui ne
  contient que JIS X 0208 et l'ASCII : **aucune lettre accentuée** (é, è, à, ç, œ…).
- Le projet de traduction garde les accents dans ses CSV mais les **replie** à la construction des fichiers
  (`é → e`, option `--fold-unsupported-chars`).
- Côté serveur, Erupe a une langue `fr` (`lang_fr.go`, commande `!lang fr`, champs `fr` dans les quêtes JSON),
  mais `stringsupport.UTF8ToSJIS` **supprime** les caractères non encodables : « Réinitialisation » arrive en
  « Rinitialisation ». Les quêtes JSON avec accents sont refusées (erreur de conversion).

**Conséquence et plan :**
1. **Phase A (sûre)** : repli cohérent `é → e` partout. Côté serveur, patch d'Erupe pour replier au lieu de
   supprimer (une fonction, testable). Résultat : français lisible sans accents.
2. **Phase B (recherche)** : retrouver la police bitmap dans les fichiers du client *(emplacement inconnu, à
   vérifier sur le client)*, redessiner une quinzaine de glyphes CP932 rarement utilisés en é è ê ë à â ç î ï ô
   û ù œ É È, et appliquer la **même table de correspondance** au client et à Erupe. Personne ne l'a encore fait
   publiquement : faisabilité non garantie.

## Ce que « intégralement en français » couvre réellement

| Zone | Source du français | Risque |
|---|---|---|
| Objets, armes, armures, compétences, interface des fichiers `dat/pac/inf/jmp/gao/sqd/rcc/msx` | MHFrontier-Translation (63 k chaînes) + compléments | longueur max ~24 demi-chasses ; codes `{j}`, `{cNN}`, `~ANN` à préserver |
| Quêtes et scénarios (envoyés par le serveur) | à produire : conversion `.bin` → JSON Erupe avec champs `fr` | gros volume ; scénarios en blocs JKR, limite 0x8000 octets |
| Messages du serveur | `lang_fr.go` d'Erupe (existe déjà) | accents (voir ci-dessus) |
| Textes codés en dur dans `mhfo-hd.dll` | patch binaire ou en mémoire | *à vérifier* : étendue inconnue |
| Images contenant du texte (titres, bannières) | retouche de textures | *à vérifier* ; probablement partiel |
| Launcher, documentation | nous | aucun |

Les compléments de traduction (chaînes manquantes, quêtes) peuvent être produits par traduction assistée,
puis relus. Le projet MHFrontier-Translation étant sous GPL-3.0, ses données restent un **téléchargement
séparé** (ou un dossier à part avec sa licence), sans être fusionnées dans le code propriétaire du launcher.

## Compatibilité avec le matériel de 2026

Le client ZZ est un jeu DirectX 9 conçu pour Windows XP à 7. Rien de spécifique à MHF n'a été trouvé en ligne
(recherche du 2026-10-27) : chaque point se vérifie sur le vrai client. Le launcher les regroupera dans un
module « FrontierFixes », sur le modèle de VanillaFixes et DXVK dans HomeCraft (activables et réversibles,
fichiers déposés seulement si leur empreinte est connue).

| Problème probable | Correctif envisagé | Statut |
|---|---|---|
| Bibliothèques D3DX9 absentes d'un Windows récent | détection par le launcher ; installateur officiel « DirectX End-User Runtimes (June 2010) », ou DLL redistribuables à côté du jeu si les conditions de redistribution le permettent | à vérifier |
| Pilotes DirectX 9 récents : saccades, plantages, Alt+Tab (Windows 11 24H2) | **DXVK** (`d3d9.dll`, zlib) : DirectX 9 traduit en Vulkan, comme dans HomeCraft ; indispensable aussi sous Wine (poste CachyOS) | à tester |
| Écrans 144 à 240 Hz : vitesse du jeu liée aux images par seconde | limiteur d'images (`d3d9.maxFrameRate` de DXVK) | à vérifier |
| Résolutions 1440p / 4K, écrans ultra-larges, mise à l'échelle Windows (DPI) | réglages de résolution du client à identifier (`mhf.ini` ?), indicateur DPI propre à l'application, plein écran fenêtré | à vérifier |
| Polices japonaises absentes d'un Windows français | détection et installation de la fonctionnalité Windows « Polices japonaises supplémentaires » (vérifiée aussi par mhf-outpost) | à vérifier |
| Paramètres régionaux non japonais (textes, chemins en CP932) | le boot stub démarre le jeu lui-même ; sinon, émulation des paramètres régionaux pour ce seul processus | à vérifier |
| **Chemins avec des caractères absents de CP932** : le dossier actuel « frontière » contient un `è` | installer le client dans un chemin ASCII (le launcher refusera ou proposera de déplacer) | **probable** |
| Manettes XInput / DualSense | prise en charge native à tester ; correspondance des touches sinon | à vérifier |
| Windows Defender signale `mhf.exe` et les boot stubs | aide à l'ajout d'une exclusion limitée au dossier du jeu, avec accord de l'utilisateur | connu (mhf-outpost) |
| GameGuard et Internet Explorer | supprimés d'office par le boot stub | réglé |

## Plan par phases

| Phase | Contenu | Testable sans client ? |
|---|---|---|
| 0 | Audit, squelette du projet (fait : `Launcher/` compile) | oui |
| 1 | PostgreSQL portable + Erupe pilotés par le launcher, création de compte par l'API, arrêt propre — **fait le 2026-10-27** (PostgreSQL 17.11, Erupe v9.4.1, premier démarrage et redémarrage testés, aucun processus orphelin) | oui |
| 2 | Lancement du client via boot stub 32 bits, entrée en jeu — **fait le 2026-10-27** : mhf-iel-cli compilé, sélection du monde, création de personnage et sauvegarde vérifiées en jeu ; quêtes installées. Ajouts : plusieurs comptes, serveur local/distant, ports réglables | non |
| 2b | Compatibilité 2026 : FrontierFixes (DXVK, D3DX9, limiteur, résolution, DPI, polices, chemin ASCII) | non |
| 3 | Traduction client : application de MHFrontier-Translation (repli des accents), mesure de couverture — **appliquée le 2026-10-28** : 63 218 chaînes (mhfdat 62 789, mhfpac 304, mhfsqd 125), relecture et restauration vérifiées ; 46 chaînes de menus en attente d'un `headers.json` à jour ; à contrôler en jeu | non |
| 4 | Traduction serveur : Erupe patché (repli), `Language: fr`, quêtes/scénarios en français — **quêtes : outil fait et pilote appliqué le 2026-10-28** (14 223 textes japonais uniques ; 132 traduits = 61 % des occurrences, 266 899 textes dans 54 951 fichiers) ; reste : traduction complète des quêtes, scénarios (145 377 fichiers), patch d'Erupe pour les accents | partiellement |
| 5 | Accents : recherche de la police, table de glyphes commune client/serveur | non |
| 6 | Fonctions HomeCraft : sauvegardes, réparation, réglages (multiplicateurs de gameplay pour le solo), package portable | oui |

## Piste : bots (20 au maximum) — étude détaillée dans `02-Etude-Bots.md`

Différence fondamentale avec les PartyBots de VMaNGOS : dans MHF, les monstres, les dégâts et la physique des
quêtes sont calculés par le **client** de l'hôte de la partie ; Erupe ne fait que relayer les paquets entre
joueurs. Un bot ne peut donc pas « jouer » côté serveur comme dans WoW. Niveaux envisageables, du plus simple
au plus lourd, à confirmer par une étude du protocole d'Erupe (`server/channelserver`) :

| Niveau | Contenu | Faisabilité estimée |
|---|---|---|
| 1 | Bots de **présence** : faux joueurs connectés au canal, visibles en ville, qui se déplacent et parlent | plausible (sessions simulées côté Erupe) |
| 2 | Bots **recrutables** : apparaissent dans les recherches de partie, rejoignent une quête | incertain |
| 3 | Bots **compagnons de chasse** qui attaquent les monstres | très lourd : il faudrait réimplémenter la logique de combat du client |

Le jeu propose déjà des compagnons contrôlés par le client en solo (Partenaire, Halk ; à vérifier sur le
client ZZ et avec Erupe), à prendre en compte avant de viser le niveau 3.

## Risques

- **Erupe archivé** : plus de correctifs en amont. On fige v9.4.1 et on garde nos patchs dans `Build/erupe-patches`.
- **Solo** : MHF est conçu pour le multijoueur et n'a pas de compagnons IA comparables aux PartyBots. Compensation
  par les réglages de gameplay d'Erupe (`config.reference.json` : multiplicateurs, cours activés par défaut).
- **Antivirus** : `mhf.exe` et les boot stubs sont souvent signalés par Windows Defender (mentionné par mhf-outpost).
- **Légal** : client et fichiers de quêtes sont des œuvres Capcom ; jamais inclus dans un livrable ni un dépôt.

## Sources

- https://github.com/Mezeporta/Erupe (README, `config.reference.json`, `docs/openapi.yaml`, `common/stringsupport/string_convert.go`, `server/channelserver/lang_fr.go`)
- https://github.com/rockisch/mhf-iel
- https://github.com/Mogapedia/mhf-outpost
- https://github.com/Mogapedia/MHFrontier-Translation (README, CHANGELOG, `scripts/build_bins.py`)
- https://github.com/Blizz127/Monster-Hunter-Frontier-PC-English-Translation
- https://github.com/mhvuze/ReFrontier
- https://www.houmgaor.com/monster-hunter-frontier/translating-monster-hunter-frontier/
- https://www.postgresql.org/download/windows/
