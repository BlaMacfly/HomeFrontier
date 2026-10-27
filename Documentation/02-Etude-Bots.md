# 02 — Étude de faisabilité : bots (20 au maximum)

*2026-10-27. Lecture des sources d'Erupe v9.4.1 (commit efec0e47, clone en lecture seule dans `Build/erupe-src`).
Rien n'a encore été testé en jeu : chaque « à vérifier » se tranche par une capture ou un essai.*

## Le point qui décide de tout

Dans MHF, le serveur **ne simule rien de la chasse**. Monstres, dégâts, morts, récompenses : tout est calculé
par le client (`mhfo-hd.dll`) et voyage entre joueurs dans des paquets « cast binaire » qu'Erupe relaie sans
les lire (`server/channelserver/handlers_cast_binary.go`). Seule exception : les PV partagés de Raviente
(`handlers_register.go`). Un bot ne peut donc pas « jouer » côté serveur comme les PartyBots de WoW : il doit
soit **imiter un client** sur le réseau, soit **utiliser une IA que le jeu possède déjà**.

## Ce qu'Erupe fournit déjà

| Élément | Où | Utilité pour les bots |
|---|---|---|
| **protbot** : client MHF sans affichage (sign → entrance → canal, chiffrement Blowfish, ACK, ping) | `cmd/protbot/` (MIT) | base directe d'un bot réseau : sait se connecter, entrer en ville (`sl1Ns200p0a0u0`), envoyer et lire le chat |
| Objets de joueur : création, position, rotation relayées | `handlers_object.go` (`CreateObject`, `PositionObject`, `RotateObject`) | déplacer un bot en ville (coordonnées X/Y/Z en clair) |
| Binaires de joueur : nom, profil (208 o), équipement (384 o) | `handlers_users.go` | apparence du bot ; formats en partie connus, à copier depuis un vrai personnage |
| Capture de paquets `.mhfr` + rejeu | option `Capture` (désactivée par défaut), `cmd/replay/` | enregistrer ce qu'envoie le vrai jeu (déplacements, animations, recrutement) pour l'imiter |
| Espion de positions | `cmd/positiontap/` | décoder les paquets de position d'un vrai client |
| Mercenaires **Rasta** : immatriculation, sauvegarde, contrat | `handlers_mercenary.go`, `repo_mercenary.go` | piste pour des **compagnons de chasse** pilotés par l'IA du jeu (voir niveau 3b) |

Ce qui n'existe pas : aucun PNJ, joueur fantôme ou commande qui crée des sessions de bot.

## Les niveaux possibles

### Niveau 1 — Bots de présence en ville (faisabilité : élevée)

20 faux joueurs connectés, visibles à Mezeporta, qui se déplacent et parlent en français.

- **Comment** : un petit programme Go (« frontier-bots ») bâti sur protbot. Chaque bot se connecte, envoie son
  nom, son profil et son équipement (`SetUserBinary` 1 à 3), entre en ville, crée son avatar (`CreateObject`),
  puis se déplace (`PositionObject` / `RotateObject`) et parle (cast Chat, format déjà implémenté).
- **Ce qui manque** :
  - des **personnages de bot valides** : sauvegarde et binaires copiés d'un vrai personnage, avec un nom changé ;
  - le format des **animations** (cast State) : partiellement documenté, à capturer sur le vrai jeu ;
  - le HR dans le binaire d'équipement est chiffré avec une clé de session (algorithme non documenté) :
    à défaut, rang affiché approximatif.
- **Intégration au launcher** : onglet « Bots » (0 à 20, noms, phrases), démarrage et arrêt avec le serveur.
- **Coût estimé** : 2 à 4 séances de travail, dont une de captures en jeu avec toi.

### Niveau 2 — Bots qui rejoignent ta quête (faisabilité : moyenne, risque élevé)

- Côté serveur, c'est simple : réserver une place dans la salle de quête, y entrer, charger le fichier de quête
  (`handlers_stage.go`, `handlers_quest.go`).
- **Le risque** : ton jeu, hôte de la quête, attend des paquets de quête que le bot devrait produire (état
  prêt, synchronisation). S'il ne les reçoit pas : blocage au départ ou désynchronisation.
- **Et même s'il entre**, le bot ne ferait rien : c'est un figurant immobile, qui compte comme un joueur dans
  la quête (monstres plus résistants) sans t'aider.
- **Coût estimé** : 3 à 6 séances, résultat incertain. **Pas recommandé.**

### Niveau 3a — Bots qui chassent vraiment, en réseau (faisabilité : très faible)

Il faudrait rétro-ingénierer tout le protocole de combat et réécrire l'IA de combat du jeu dans le bot.
Plusieurs mois, sans garantie. **Déconseillé.**

### Niveau 3b — Compagnons de chasse via les Rasta (faisabilité : à vérifier, potentiellement bonne)

Le jeu a déjà des **chasseurs IA** : les **Rasta**, des personnages de joueurs immatriculés comme mercenaires
qu'un autre joueur engage, et que **son propre jeu** fait combattre à ses côtés. Erupe gère tout le circuit :
immatriculation (`CreateMercenary` → `rasta_id`), sauvegarde des données de mercenaire produites par le jeu
(`SaveMercenary` → colonne `savemercenary`), contrat (`ContractMercenary` → `pact_id`), lecture
(`ReadMercenaryW/M`).

- **Idée** : créer des personnages de bot immatriculés en Rasta, avec des données de mercenaire copiées d'un
  vrai Rasta (le tien, immatriculé une fois en jeu) et des noms ou équipements différents. Tu les engages, et
  c'est l'IA de Capcom qui chasse : aucune IA à écrire.
- **Limites connues dans le code** :
  - un seul contrat à la fois par personnage (`pact_id` est une seule valeur) ;
  - aucune liste « Rasta disponibles » côté serveur : le jeu en engage un via un personnage précis, ce qui
    rend plausible d'engager **un bot croisé en ville** (lien avec le niveau 1).
- **À vérifier en jeu** :
  - comment on trouve et engage un Rasta dans le client ZZ ;
  - combien de compagnons on peut emmener par quête ;
  - si des données copiées sont acceptées.
- **Coût estimé** : 1 séance d'essais (captures), puis 2 à 3 séances si le principe tient.

À voir aussi : le **Partenaire** et le **Halk**, compagnons IA solo déjà gérés par Erupe (sauvegardes
`partner`, `otomoairou`). Ils sont peut-être suffisants pour le solo, sans aucun développement.

## Prérequis communs

- **Go 1.25** sur le poste : pour compiler protbot et le module de bots. Ce sera aussi nécessaire pour patcher
  Erupe (repli des accents, phase 4).
- **Captures** : activer `Capture` dans la configuration générée, jouer quelques minutes (marcher en ville,
  immatriculer et engager un Rasta), puis analyser les fichiers `.mhfr`.
- **Comptes de bots** : créés par l'API, avec des personnages initialisés à partir d'un modèle. Ils sont
  marqués comme bots pour ne pas apparaître dans l'onglet Comptes.

## Recommandation

1. **D'abord une séance d'essais** (sans code) : captures en jeu, test du Partenaire, du Halk et des Rasta. On
   saura alors si le niveau 3b est possible, et le niveau 1 aura ses formats.
2. **Ensuite le niveau 1**, présence en ville avec 20 bots au maximum : faisabilité élevée, coût maîtrisé.
3. **Le niveau 3b** si les essais le confirment : c'est le seul chemin réaliste vers des bots qui chassent.
4. **Écarter les niveaux 2 et 3a.**
