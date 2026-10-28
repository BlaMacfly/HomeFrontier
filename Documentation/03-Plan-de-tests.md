# Plan de tests en jeu — HomeFrontier

*Préparé le 2026-10-28. À dérouler dans l'ordre ; noter pour chaque point « OK », « KO » + capture d'écran, ou « non
trouvé ». Installation de test : `C:\Users\Marty\HomeFrontier`.*

## 0. Avant de commencer

- Base sauvegardée : `UserData\Sauvegardes\Database-2026-10-28-avant-tests` (copie vérifiée identique). Pour revenir
  en arrière : launcher fermé, remplacer `UserData\Database` par cette copie.
- Lancer `HomeFrontier.exe`, onglet **Jouer** → démarrer le serveur. Il charge les quêtes et scénarios traduits ainsi
  qu'Erupe recompilé (accents repliés). Le binaire d'origine reste dans `Server\erupe\erupe-ce-officiel.exe`.
- Onglet **Jeu** → « Vérifications » : tout doit être vert.
- En cas de souci : `Logs\launcher.log`, `Logs\erupe.log`, `Logs\mhf-iel.log` (et `Logs\*d3d9.log` avec DXVK).

## 1. Affichage et compatibilité (onglet Jeu)

| # | Réglage | À observer |
|---|---|---|
| 1.1 | Rien de changé (DXVK coupé) | le jeu démarre comme avant : référence |
| 1.2 | **Fenêtré** + 1600 × 900 | fenêtre de 1600 × 900 ? (sinon `FULLSCREEN_MODE` a le sens inverse : le noter) |
| 1.3 | **Plein écran** + résolution de l'écran | plein écran net, pas d'image étirée |
| 1.4 | « Ignorer la mise à l'échelle de Windows » (si Windows est à 125 % ou plus) | image plus nette ; souris toujours alignée |
| 1.5 | **DXVK** activé | le jeu démarre ; fichier `Logs\mhf-iel-cli_d3d9.log` créé ; fluidité, Alt+Tab |
| 1.6 | DXVK + limite à 60 images/s | jeu pas accéléré ni ralenti (animations, chronomètre) |
| 1.7 | DXVK désactivé à nouveau | `Tools\mhf-iel\d3d9.dll` disparu, le jeu redémarre normalement |

## 2. Textes du serveur

| # | Où | À observer |
|---|---|---|
| 2.1 | Comptoir des quêtes : ≪Bases du chasseur≫ « Les bases des bases » (quête 1) | **règle graduée** dans la description : noter le dernier chiffre visible sur chaque ligne et le nombre de lignes affichées |
| 2.2 | Même catégorie, « L'art de griller la viande » (quête 2) | idem ; objectif « Livrer 1 steak bien cuit » |
| 2.3 | Quelques quêtes au hasard (Jungle, Désert, quêtes d'événement) | titres et catégories en français ; objectifs du type « Chasser 1 Rathalos » ; noter les textes restés en japonais ou absurdes |
| 2.4 | Liste des histoires / scénarios | la description qui commençait par « メゼポルタに怪異譚が流布している » affiche une **règle 40/44/48** : noter ce qui est coupé |
| 2.5 | Dialogue d'un PNJ de la guilde (accepter une quête, refuser, boîte pleine…) | français, pages pas trop longues, `@MYNAME` remplacé par ton nom de chasseur |
| 2.6 | Une histoire avec des Félynes (scripts de dialogue compressés) | la cinématique **se déroule jusqu'au bout** sans blocage ; bulles en français |
| 2.7 | Messages système d'Erupe (connexion, chat `!help` s'il existe) | accents repliés (« desactivee ») et plus de lettres manquantes (« dsactive ») |

Si une cinématique bloque : fermer le jeu, puis `HomeFrontier.exe --do scenario-untranslate` remet les
scénarios d'origine (le noter, c'est une information importante pour la suite).

## 3. Interface du client (traduction MHFrontier-Translation)

| # | Où | À observer |
|---|---|---|
| 3.1 | Menus, objets, compétences | français sans accents ; noter les menus restés en anglais (46 chaînes connues) |

## 4. Compagnons (piste « playerbots »)

| # | Où | À observer |
|---|---|---|
| 4.1 | **Partenaire** (パートナー) | où on le crée, s'il chasse avec toi, jusqu'à quel point il est réglable |
| 4.2 | **Halk** | idem |
| 4.3 | **Rasta** (mercenaires) | où on immatricule son propre personnage, où on en engage un, combien peuvent venir en quête |

Ces observations décident de la suite du module de bots (`Documentation/02-Etude-Bots.md`, niveau 3b).

## 5. Après les tests

Rapporter : la largeur et le nombre de lignes lus sur les règles (2.1, 2.2, 2.4), le sens de `FULLSCREEN_MODE`
(1.2), le résultat de DXVK (1.5), le déroulement de la cinématique (2.6) et les captures des textes ratés. Les
découpages (`--largeur`, lignes par page) seront ajustés en conséquence, sans retraduire.
