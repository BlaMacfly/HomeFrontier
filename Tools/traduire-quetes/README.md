# Traducteur hors ligne des quêtes

Traduit automatiquement en français les textes de quêtes de Monster Hunter Frontier (japonais), **sur ton PC**,
sans compte ni service en ligne une fois installé. Moteur : [NLLB-200](https://huggingface.co/facebook/nllb-200-distilled-600M)
de Meta (licence CC-BY-NC 4.0, usage non commercial), exécuté par [CTranslate2](https://github.com/OpenNMT/CTranslate2).

## Installation (une fois)

Prérequis : Python 3.10 à 3.12 (`winget install -e --id Python.Python.3.12`).

```bat
Installer.cmd -Modele 600M
```

Crée un environnement Python privé (`.venv`), installe CTranslate2, télécharge et convertit le modèle
(`modele-nllb-600M`, environ 600 Mo). `-Modele 1.3B` : meilleure qualité, deux fois plus lent, 8 Go de mémoire.

## Traduire

Launcher et jeu fermés :

```bat
Traduire.cmd -Racine C:\Jeux\HomeFrontier
```

1. Inventaire des textes japonais (`Localization\quetes-source.tsv`, produit par le launcher s'il manque).
2. Traduction → `Localization\quetes-fr-auto.tsv` (reprise automatique après une interruption).
3. Application aux quêtes par le launcher (`--do quest-translate`). Redémarrer ensuite le serveur.

Options : `-Limite 50` (essai), `-Largeur 26` (caractères par ligne), `-SansAppliquer`, `-Modele 1.3B`.

## Qualité

- Les traductions faites à la main (`Localization\quetes-fr.tsv`) ne sont jamais retraduites et restent
  prioritaires sur les traductions automatiques.
- Noms de monstres, surnoms (火竜 → Rathalos, 雪獅子 → Blangonga) et armes : `glossaire.tsv` (japonais →
  français), remplacés avant traduction.
- Catégories des titres (≪イベントクエスト≫, ≪Ｇ★７フロンティア≫, ≪辿異★４クエスト≫…) : `etiquettes.tsv`,
  préfixes G★n / 辿異★n composés automatiquement.
- Objectifs à formule (« X１頭の狩猟 », « X２個の納品 », « Xの尻尾切断 », « Xの角破壊 »…) et structures de titre
  courantes (« Xのなわばり », « Xを極めし者 », 【２人向け】…) : traduits par règles.
- Sur un texte court sans verbe, NLLB invente souvent une phrase toute faite (« Je ne sais pas ce que c'est ») :
  elle est écartée et le japonais d'origine gardé (signalé « japonais restant » dans le rapport). Dans une
  description, une telle formule absente du japonais fait choisir une autre proposition du modèle.
- Textes à revoir (lignes trop longues, japonais restant, codes de touches) : `quetes-fr-auto-rapport.tsv`.
  Une correction se fait en ajoutant la ligne corrigée dans `quetes-fr.tsv`.
