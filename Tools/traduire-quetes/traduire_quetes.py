"""Traduction automatique hors ligne des textes de quêtes de Monster Hunter Frontier (japonais → français).

Moteur : NLLB-200 (Meta) exécuté localement par CTranslate2, sans compte ni connexion une fois le modèle installé
(voir Installer.ps1). Entrée : l'inventaire produit par le launcher (`HomeFrontier.exe --do quest-inventory`),
colonnes source / occurrences / champs. Sortie : `quetes-fr-auto.tsv` (colonnes source, fr), que le launcher
applique avec une priorité plus basse que les traductions faites à la main (`quetes-fr.tsv`).

Ce qu'un modèle généraliste traduit mal est traité avant ou après lui :
  - noms de monstres : remplacés par leur nom français (glossaire.tsv) avant traduction ;
  - objectifs à formule (« X１頭の狩猟 », « X２個の納品 »…) : traduits par règles, sans le modèle ;
  - mise en page : les retours à la ligne japonais sont de simple mise en forme ; le texte est traduit phrase par
    phrase puis recoupé à `--largeur` caractères par ligne ; les textes qui dépassent sont listés dans le rapport.

Le travail est repris là où il s'est arrêté : les textes déjà présents dans la sortie sont sautés.
"""

from __future__ import annotations

import argparse
import re
import sys
import time
import unicodedata
from pathlib import Path

ICI = Path(__file__).resolve().parent

# ── Échappement TSV (même convention que le launcher : \n, \t, \r, \\) ─────────────────────────────────────────


def unescape(s: str) -> str:
    out, it = [], iter(s)
    for c in it:
        if c != "\\":
            out.append(c)
            continue
        n = next(it, "")
        out.append({"n": "\n", "r": "\r", "t": "\t"}.get(n, n if n else "\\"))
    return "".join(out)


def escape(s: str) -> str:
    return s.replace("\\", "\\\\").replace("\t", "\\t").replace("\r", "\\r").replace("\n", "\\n")


def read_tsv(path: Path) -> tuple[list[str], list[list[str]]]:
    lines = path.read_text(encoding="utf-8-sig").splitlines()
    if not lines:
        return [], []
    return lines[0].split("\t"), [l.split("\t") for l in lines[1:] if l]


# ── Glossaire et règles ───────────────────────────────────────────────────────────────────────────────────────

SUFFIXES = {
    "希少種": "rare",
    "亜種": "sous-espèce",
    "変種": "variante",
    "奇種": "variante rare",
    "剛種": "HC",
    "覇種": "Suprématie",
    "辿異種": "Zénith",
    "特異個体": "singulier",
    "極み個体": "extrême",
    "繁殖期": "en rut",
    "激昂": "enragé",
}

ACTIONS = {"狩猟": "Chasser", "討伐": "Tuer", "捕獲": "Capturer", "納品": "Livrer", "撃退": "Repousser", "撃破": "Vaincre"}


def load_glossary(path: Path) -> list[tuple[str, str]]:
    _, rows = read_tsv(path)
    entries = [(r[0], r[1]) for r in rows if len(r) >= 2 and r[0]]
    # Les plus longs d'abord : « リオレウス亜種 » avant « リオレウス ».
    return sorted(entries, key=lambda e: len(e[0]), reverse=True)


def normalize(text: str) -> str:
    """Chiffres et lettres pleine chasse → ASCII (１ → 1, Ｒ → R), sans toucher aux ≪ ≫ ni aux ～."""
    out = []
    for c in text:
        if c in "≪≫～":
            out.append(c)
        else:
            out.append(unicodedata.normalize("NFKC", c))
    return "".join(out)


def apply_glossary(text: str, glossary: list[tuple[str, str]]) -> str:
    for jp, fr in glossary:
        if jp in text:
            text = text.replace(jp, f" {fr} ")
    # Suffixes restés accolés à un nom traduit : « Velociprey 変種 » → « Velociprey variante ».
    for jp, fr in SUFFIXES.items():
        text = re.sub(rf"([A-Za-zÀ-ÿ\-\.]+)\s*{jp}", rf"\1 {fr}", text)
    return re.sub(r"\s+", " ", text).strip()


JAPANESE = re.compile(r"[\u3040-\u30ff\u3400-\u9fff]")
FORMULA = re.compile(r"^(?P<cible>.+?)\s*(?P<n>\d+)\s*(?:頭|体|匹|個|本|枚|つ|羽|尾)の(?P<action>狩猟|討伐|捕獲|納品|撃退|撃破)$")
COUNT_ONLY = re.compile(r"^(?P<cible>.+?)\s*(?P<n>\d+)\s*(?:頭|体|匹)$")
# Objectifs secondaires : « Xの尻尾切断 », « Xの角破壊 », « Xの一定ダメージ ».
PART = re.compile(r"^(?P<x>.+?)\s*の(?P<part>.+?)(?P<verb>破壊|切断)$")
PARTS = {
    "尻尾": "la queue", "頭": "la tête", "頭部": "la tête", "角": "les cornes", "翼": "les ailes", "両翼": "les ailes",
    "左翼": "l'aile gauche", "右翼": "l'aile droite", "前脚": "les pattes avant", "後脚": "les pattes arrière",
    "脚": "les pattes", "背中": "le dos", "爪": "les griffes", "牙": "les crocs", "胸": "le poitrail", "胸部": "le poitrail", "腕": "les bras",
    "腹部": "le ventre", "背": "le dos", "背部": "le dos", "尾": "la queue", "頭殻": "le crâne", "前足": "les pattes avant",
    "甲殻": "la carapace", "背甲": "la carapace", "ヤド": "la coquille", "嘴": "le bec", "耳": "les oreilles",
    "顔": "le visage", "首": "le cou", "腹": "le ventre", "棘": "les épines", "鎌": "les faux", "ハサミ": "les pinces",
}
DAMAGE = re.compile(r"^(?P<x>.+?)\s*(?:への|の|へ)一定ダメージ$")

# ── Moteur de traduction ──────────────────────────────────────────────────────────────────────────────────────


class Engine:
    def __init__(self, model_dir: Path, beam: int, threads: int):
        import ctranslate2
        from transformers import AutoTokenizer

        self.tok = AutoTokenizer.from_pretrained(str(model_dir), src_lang="jpn_Jpan")
        self.tr = ctranslate2.Translator(str(model_dir), device="cpu", compute_type="int8", intra_threads=threads)
        self.beam = beam
        self.cache: dict[str, str] = {}

    def translate(self, segments: list[str]) -> list[str]:
        todo = [s for s in dict.fromkeys(segments) if s not in self.cache]
        for i in range(0, len(todo), 32):
            batch = todo[i : i + 32]
            tokens = [self.tok.convert_ids_to_tokens(self.tok.encode(s)) for s in batch]
            results = self.tr.translate_batch(
                tokens,
                target_prefix=[["fra_Latn"]] * len(batch),
                beam_size=self.beam,
                max_decoding_length=256,
                repetition_penalty=1.2,
            )
            for src, res in zip(batch, results):
                hyp = res.hypotheses[0][1:]  # sans le jeton de langue
                self.cache[src] = self.tok.decode(self.tok.convert_tokens_to_ids(hyp), skip_special_tokens=True).strip()
        return [self.cache[s] for s in segments]

    def candidates(self, segment: str, n: int) -> list[str]:
        """Les `n` meilleures traductions d'un segment (pour écarter une phrase inventée par le modèle)."""
        tokens = self.tok.convert_ids_to_tokens(self.tok.encode(segment))
        res = self.tr.translate_batch(
            [tokens], target_prefix=[["fra_Latn"]], beam_size=max(self.beam, n), num_hypotheses=n,
            max_decoding_length=256, repetition_penalty=1.2,
        )[0]
        return [self.tok.decode(self.tok.convert_tokens_to_ids(h[1:]), skip_special_tokens=True).strip() for h in res.hypotheses]


# Sur un texte court sans verbe (titre, nom d'objet), NLLB invente souvent une phrase toute faite (« Je ne sais pas
# ce que c'est », « Il y a… », « Je vous en prie »). Une telle sortie est écartée et le japonais d'origine conservé.
INVENTED_SHORT = re.compile(
    r"^\W*(?:je ne sais pas|je ne suis pas|je vous en prie|quoi qu'il en soit|il y a |il s'agit|c'est |ce n'est|"
    r"j'ai |je suis |je l'ai|je vais|je |nous |vous |tu |ils |elles |il a |oh\b|merci|bonjour|s'il vous plaît)",
    re.IGNORECASE,
)
# Dans une phrase, ces formules ne sont acceptées que si le japonais contient ce qu'elles traduisent.
INVENTED_SENTENCE = [
    (re.compile(r"je ne sais pas|je ne suis pas sûr", re.IGNORECASE), re.compile(r"知ら|知る|知れ|わから|分から|判ら|わかん|分かん|不明|謎")),
    (re.compile(r"je vous en prie", re.IGNORECASE), re.compile(r"どうぞ|お願|願い|ください|下さい")),
]


def invented(fr: str, jp: str) -> bool:
    return any(out.search(fr) and not cue.search(jp) for out, cue in INVENTED_SENTENCE)


# ── Découpage, traduction, mise en page ────────────────────────────────────────────────────────────────────────

CODE = re.compile(r"\{[A-Za-z]\d+\}|\{/?c\d*\}|~C\d\d")
SENTENCE_END = re.compile(r"(?<=[。！？!?])")


def sentences(paragraph: str) -> list[str]:
    return [s.strip() for s in SENTENCE_END.split(paragraph) if s.strip()]


FIXES = [
    # Le modèle laisse « Guild » en anglais et au masculin.
    (re.compile(r"\b[Dd]u Guild\b"), "de la guilde"),
    (re.compile(r"\b[Aa]u Guild\b"), "à la guilde"),
    (re.compile(r"\b[Ll]e Guild\b"), "la guilde"),
    (re.compile(r"\bGuilds?\b"), "guilde"),
]


def finish(text: str) -> str:
    """Retouches du texte produit : vocabulaire, guillemets et tilde compatibles avec le jeu, espaces."""
    for pattern, repl in FIXES:
        text = pattern.sub(repl, text)
    text = text.replace("~", "～").replace("«", "≪").replace("»", "≫")
    text = re.sub(r"≪\s+", "≪", text)
    text = re.sub(r"\s+≫", "≫", text)
    return re.sub(r"\s+", " ", text).strip()


def wrap(text: str, width: int) -> list[str]:
    # La ponctuation double française (« ? », « ! », « : », « ; ») reste collée au mot qui la précède.
    text = re.sub(r" (?=[?!:;](?:\s|$))", "\x00", text)
    return [l.replace("\x00", " ") for l in _wrap(text, width)]


def _wrap(text: str, width: int) -> list[str]:
    lines, cur = [], ""
    for word in text.split(" "):
        while len(word) > width:  # mot plus long qu'une ligne : coupé net
            if cur:
                lines.append(cur)
                cur = ""
            lines.append(word[:width])
            word = word[width:]
        if not cur:
            cur = word
        elif len(cur) + 1 + len(word) <= width:
            cur += " " + word
        else:
            lines.append(cur)
            cur = word
    if cur:
        lines.append(cur)
    return lines


def load_tags(path: Path) -> dict[str, str]:
    """Étiquettes de catégorie des titres (≪…≫) : japonais normalisé, nombres remplacés par {n}."""
    if not path.is_file():
        return {}
    _, rows = read_tsv(path)
    return {normalize(r[0]): r[1] for r in rows if len(r) >= 2 and r[0]}


NUMBER = re.compile(r"\d+")
TAG_PREFIX = re.compile(r"^(?P<ruines>古跡・)?(?:G★(?P<g>\d+)|辿異★(?P<z>\d+))?(?P<core>.*)$")
TITLE_ACTION = re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*の?(?P<a>狩猟|討伐|捕獲|撃退|撃破)(?:せよ)?$")
TITLE_PATTERNS = [
    (re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*を(?P<a>狩猟|討伐|捕獲|撃退|撃破)せよ$"), None),
    (re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*のなわばり$"), "Le territoire : {x}"),
    (re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*(?:襲来|現る|出現)$"), "{x} attaque"),
    (re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*連続狩猟$"), "Chasse en série : {x}"),
    (re.compile(r"^みんなで\s*(?P<x>[^぀-ヿ㐀-鿿]+)$"), "Tous ensemble : {x}"),
    (re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*を極めし者(?:たち)?$"), "Maître : {x}"),
    (re.compile(r"^(?P<x>[^぀-ヿ㐀-鿿]+?)\s*[と&]\s*(?P<y>[^぀-ヿ㐀-鿿]+)$"), "{x} et {y}"),
]
BRACKET = re.compile(r"【([^】]*)】")
# Variables des scénarios, remplacées pendant la traduction par un mot que le modèle recopie tel quel.
VARIABLES = {"@MYNAME": "Mathis", "@MYCLEARNUM": "7777", "@MYMONEY": "8888", "@CHAHR": "9999"}
# Lignes par page de dialogue (les pages japonaises en ont 3 ou 4 ; à confirmer en jeu).
PAGE_LINES = 4
PLAYERS = re.compile(r"^(\d+)人(?:向け|用|専用)?$")


class Translator:
    def __init__(self, engine: Engine | None, glossary: list[tuple[str, str]], width: int, tags: dict[str, str] | None = None):
        self.engine, self.glossary, self.width, self.tags = engine, glossary, width, tags or {}

    def mt(self, texts: list[str]) -> list[str]:
        if self.engine is None:
            return texts
        return self.engine.translate(texts)

    def short(self, text: str, fallback: str) -> str:
        """Texte court (fragment de titre, nom d'objet) : traduction du modèle, sauf phrase inventée → `fallback`."""
        if not JAPANESE.search(text):
            return text
        out = finish(self.mt([text])[0].rstrip(".!"))
        if self.engine is None or not out or INVENTED_SHORT.search(out):
            return fallback
        return out

    def sentence(self, text: str) -> str:
        """Phrase d'une description : si le modèle invente une ignorance absente du japonais, autre proposition."""
        out = self.mt([text])[0]
        if self.engine is not None and invented(out, text):
            for alt in self.engine.candidates(text, 4)[1:]:
                if not invented(alt, text):
                    return alt
            if len(JAPANESE.findall(text)) <= 4:
                return ""  # interjection (« うむ！ ») que le modèle ne sait que transformer en formule : omise
        return out

    def by_rule(self, text: str) -> str | None:
        flat = apply_glossary(normalize(text.replace("\n", "")), self.glossary)
        m = FORMULA.match(flat)
        if m:
            cible = m["cible"].strip()
            cible = self.short(cible, cible)
            return f"{ACTIONS[m['action']]} {m['n']} {cible}"
        m = PART.match(flat)
        if m and m["part"] in PARTS and not JAPANESE.search(m["x"]):
            verb = "Couper" if m["verb"] == "切断" else "Briser"
            return f"{verb} {PARTS[m['part']]} de {m['x'].strip()}"
        m = DAMAGE.match(flat)
        if m:
            x = m["x"].strip()
            return f"Infliger assez de dégâts à {self.short(x, x)}"
        m = COUNT_ONLY.match(flat)
        if m and not JAPANESE.search(m["cible"]):
            return f"{m['n']} {m['cible'].strip()}"
        if not JAPANESE.search(flat):
            return flat  # plus rien à traduire (« ??? », noms seuls…)
        return None

    def tag(self, inner: str) -> str:
        """Étiquette ≪…≫ : table `etiquettes.tsv` (préfixes G★n, 辿異★n et 古跡・ composés), sinon modèle."""
        norm = normalize(inner)
        m = TAG_PREFIX.match(norm)
        core = m["core"]
        numbers = NUMBER.findall(core)
        core_fr = self.tags.get(NUMBER.sub("{n}", core))
        if core_fr is not None:
            for n in numbers:
                core_fr = core_fr.replace("{n}", n, 1)
        elif not core:
            core_fr = ""
        else:
            glossed = apply_glossary(core, self.glossary)
            core_fr = self.short(glossed, core)
        if m["z"]:
            core_fr = f"Quête Zénith ★{m['z']}" if core_fr in ("", "Quête") else f"Zénith ★{m['z']} : {core_fr}"
        elif m["g"]:
            core_fr = f"G★{m['g']} {core_fr}".strip()
        if m["ruines"]:
            core_fr = f"Ruines : {core_fr}"
        return core_fr

    def bracket(self, inner: str) -> str:
        norm = normalize(inner)
        players = PLAYERS.match(norm)
        if players:
            return f"{players[1]} joueurs"
        if norm in self.tags:
            return self.tags[norm]
        glossed = apply_glossary(norm, self.glossary)
        for jp, fr in ACTIONS.items():
            if glossed == jp:
                return fr
        return self.short(glossed, norm)

    def name(self, line: str) -> str:
        """Nom d'une quête (2e ligne du titre) : glossaire, crochets 【…】, structures courantes, puis modèle ;
        si le modèle invente une phrase, le japonais d'origine est gardé."""
        norm = normalize(line)
        brackets = [f"[{self.bracket(b)}]" for b in BRACKET.findall(norm)]
        main = BRACKET.sub(" ", norm).strip()
        bang = "!" if re.search(r"[!！]$", main) else ""
        main = re.sub(r"[!！]+$", "", main).strip()
        glossed = apply_glossary(main, self.glossary)
        fr = None
        m = TITLE_ACTION.match(glossed)
        if m:
            fr = f"{ACTIONS[m['a']]} {m['x'].strip()}"
        for pattern, template in TITLE_PATTERNS:
            if fr is not None:
                break
            m = pattern.match(glossed)
            if m:
                fr = f"{ACTIONS[m['a']]} {m['x'].strip()}" if template is None else template.format(**{k: v.strip() for k, v in m.groupdict().items()})
        if fr is None:
            fr = self.short(glossed, main) if glossed else ""
        return finish(" ".join(p for p in (fr + bang, *brackets) if p))

    def title(self, text: str) -> str:
        """Titres : chaque ligne a son sens propre (catégorie ≪…≫ puis nom) ; traduites séparément."""
        out = []
        for line in text.split("\n"):
            tag = re.fullmatch(r"\s*≪(.+?)≫(.*)", line)
            if tag:
                rest = self.name(tag[2]) if tag[2].strip() else ""
                out.append(finish(f"≪{self.tag(tag[1])}≫ {rest}"))
            elif line.strip():
                out.append(self.name(line))
            else:
                out.append(line)
        return "\n".join(out)

    def paragraph(self, text: str) -> str:
        """Descriptions et textes longs : lignes japonaises réunies, traduction phrase par phrase, recoupage."""
        flat = normalize(text.replace("\n", ""))
        codes = CODE.findall(flat)
        flat = CODE.sub(" ", flat)
        parts = [apply_glossary(s, self.glossary) for s in sentences(flat)]
        translated = [self.sentence(p) if JAPANESE.search(p) else p for p in parts]
        body = finish(" ".join(translated))
        if codes:  # codes de touches / couleurs : remis en tête, à vérifier (signalé dans le rapport)
            body = " ".join(codes) + " " + body
        return "\n".join(wrap(body, self.width))

    def dialogue(self, text: str, script: bool = False) -> str:
        """Scénarios (dialogues des PNJ, histoires) : pages séparées par @RETURN, chacune traduite et recoupée ;
        variables du jeu (@MYNAME…) protégées ; couleurs gardées seulement quand elles entourent toute la page."""
        # Répliques des scripts de dialogue (`script`, chunks compressés) : lignes séparées par \r\n, une bulle par
        # réplique, pas de @RETURN connu dans ce format : on ne coupe pas en pages.
        crlf = script or "\r" in text
        if crlf:
            text = text.replace("\r\n", "\n").replace("\r", "\n")
        pages = []
        for page in text.split("@RETURN"):
            if not JAPANESE.search(page):
                pages.append(page)
                continue
            lead = page[: len(page) - len(page.lstrip("\n"))]
            trail = page[len(page.rstrip("\n")) :]
            lines = [l for l in page.split("\n") if l.strip()]
            indent = "　" if lines and all(l.startswith("　") for l in lines) else ""
            body = page.strip("\n").replace("　", " ").strip()
            body = re.sub(r"@MYNAME\s*(?:さん|ちゃん|くん|君|様|殿)", "@MYNAME", body)
            # Tic de langage des Félynes (« ニャ ») : le modèle en fait un prénom (« Nina »). « ニャん » = « なん ».
            body = re.sub(r"ニャ(?=[ぁ-ゖ])(?![ぁぃぅぇぉっー])", "な", body)
            body = re.sub(r"ニャ[ぁあーッっ]*(?![ァ-ヺ])", "", body)
            wrapper = re.fullmatch(r"(~C\d\d)(.*)~C00", body, re.S)
            if wrapper and "~C" not in wrapper[2]:
                color, body = wrapper[1], wrapper[2]
            else:
                color = ""
            body = re.sub(r"~C\d\d", "", body)
            stand_ins = {}
            for var, stand_in in VARIABLES.items():
                if var in body:
                    body = body.replace(var, stand_in)
                    stand_ins[var] = stand_in
            # Après les variables (@CHAHR) : sinon le modèle lit « HR » comme « ressources humaines ».
            body = re.sub(r"[ＨH][ＲR](?![A-ZＡ-Ｚ])", "rang de chasseur", body)
            chapter = re.fullmatch(r"【([^】]*)】", body.strip())
            if chapter:  # titre de chapitre : crochets japonais gardés
                fr = f"【{self.name(chapter[1])}】"
            else:
                fr = self.paragraph(body)
            lost = False
            for var, stand_in in stand_ins.items():
                if stand_in not in fr:
                    lost = True
                fr = fr.replace(stand_in, var)
            if lost or not fr.strip():  # variable perdue par le modèle : la page reste en japonais plutôt que fausse
                pages.append(page)
                continue
            out = [indent + l for l in fr.split("\n")]
            if color:
                out[0] = color + out[0]
                out[-1] = out[-1] + "~C00"
            # Une page plus longue que la boîte de dialogue est coupée en plusieurs pages.
            limit = len(out) if crlf else max(PAGE_LINES, len(lines))
            parts = ["\n".join(out[i : i + limit]) for i in range(0, len(out), limit)]
            pages.append(lead + ("@RETURN" + lead).join(parts) + trail)
        result = "@RETURN".join(pages)
        return result.replace("\n", "\r\n") if crlf else result

    def translate(self, text: str, fields: str) -> str:
        # Un même texte peut servir à plusieurs champs (« champs » de l'inventaire) : titre d'abord, puis règles
        # d'objectifs, et jamais de règles sur une description.
        if not JAPANESE.search(text):
            return text  # déjà en anglais ou sans texte (quelques quêtes du client d'origine) : inchangé
        if "scenario" in fields:
            return self.dialogue(text, script="scenario_dialogue" in fields)
        if "titre" in fields:
            return self.title(text)
        rule = self.by_rule(text) if "description" not in fields else None
        if rule is not None:
            return finish(rule)
        if "\n" in text or len(text) > 20:
            return self.paragraph(text)
        return self.name(text)  # texte court (commanditaire, nom) : mêmes règles qu'un nom de quête


# ── Programme principal ───────────────────────────────────────────────────────────────────────────────────────


def keep_awake() -> None:
    """Empêche la mise en veille automatique de Windows tant que le script tourne (levé à sa sortie, aucun réglage
    du système n'est modifié). Sans effet ailleurs que sous Windows."""
    if sys.platform != "win32":
        return
    import ctypes

    ES_CONTINUOUS, ES_SYSTEM_REQUIRED = 0x80000000, 0x00000001
    ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)


def main() -> int:
    keep_awake()
    sys.stdout.reconfigure(errors="replace")  # console non UTF-8 : un « → » ne doit pas arrêter le script
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--source", type=Path, required=True, help="inventaire des quêtes (quetes-source.tsv)")
    p.add_argument("--manuel", type=Path, help="traductions faites à la main (quetes-fr.tsv), jamais retraduites")
    p.add_argument("--sortie", type=Path, required=True, help="fichier produit (quetes-fr-auto.tsv), complété à chaque reprise")
    p.add_argument("--modele", type=Path, default=ICI / "modele-nllb-600M", help="modèle CTranslate2 converti")
    p.add_argument("--glossaire", type=Path, default=ICI / "glossaire.tsv")
    p.add_argument("--etiquettes", type=Path, default=ICI / "etiquettes.tsv", help="catégories ≪…≫ des titres")
    p.add_argument("--largeur", type=int, default=26, help="caractères par ligne dans le jeu")
    p.add_argument("--faisceau", type=int, default=4, help="largeur du faisceau (qualité / vitesse)")
    p.add_argument("--fils", type=int, default=0, help="fils de calcul (0 : automatique)")
    p.add_argument("--limite", type=int, default=0, help="ne traduire que N textes (essai)")
    p.add_argument("--sans-modele", action="store_true", help="n'appliquer que glossaire et règles (essai sans modèle)")
    args = p.parse_args()

    header, rows = read_tsv(args.source)
    col = {name: i for i, name in enumerate(header)}
    src_i, fields_i = col["source"], col.get("champs")
    occ_i = col.get("occurrences")

    skip: set[str] = set()
    if args.manuel and args.manuel.is_file():
        h, r = read_tsv(args.manuel)
        hi = {n: i for i, n in enumerate(h)}
        skip |= {unescape(x[hi["source"]]) for x in r if len(x) > hi["fr"] and x[hi["fr"]].strip()}
    if args.sortie.is_file():
        h, r = read_tsv(args.sortie)
        hi = {n: i for i, n in enumerate(h)}
        skip |= {unescape(x[hi["source"]]) for x in r if len(x) > hi["fr"]}
    else:
        args.sortie.parent.mkdir(parents=True, exist_ok=True)
        args.sortie.write_text("source\tfr\n", encoding="utf-8")

    todo = [(unescape(r[src_i]), r[fields_i] if fields_i is not None and len(r) > fields_i else "") for r in rows]
    todo = [(s, f) for s, f in todo if s.strip() and s not in skip]
    if args.limite:
        todo = todo[: args.limite]
    total_occ = sum(int(r[occ_i]) for r in rows if occ_i is not None and len(r) > occ_i and r[occ_i].isdigit())
    print(f"{len(todo)} texte(s) à traduire ({len(skip)} déjà faits ou manuels), {total_occ} occurrences au total")
    if not todo:
        return 0

    engine = None
    if not args.sans_modele:
        if not (args.modele / "model.bin").is_file():
            print(f"Modèle introuvable : {args.modele} (lancez d'abord Installer.ps1)", file=sys.stderr)
            return 2
        print(f"Chargement du modèle {args.modele.name}…")
        engine = Engine(args.modele, args.faisceau, args.fils)
    tr = Translator(engine, load_glossary(args.glossaire), args.largeur, load_tags(args.etiquettes))

    report = args.sortie.with_name(args.sortie.stem + "-rapport.tsv")
    started, done = time.time(), 0
    with args.sortie.open("a", encoding="utf-8", newline="\n") as out, report.open("a", encoding="utf-8", newline="\n") as rep:
        if report.stat().st_size == 0:
            rep.write("source\tfr\tprobleme\n")
        for src, fields in todo:
            try:
                fr = tr.translate(src, fields)
            except Exception as e:  # un texte en échec ne doit pas arrêter les 14 000 autres
                rep.write(f"{escape(src)}\t\terreur : {e}\n")
                continue
            out.write(f"{escape(src)}\t{escape(fr)}\n")
            problems = []
            lines = fr.split("\n")
            if any(len(l) > args.largeur for l in lines):
                problems.append("ligne trop longue")
            if len(lines) > max(src.count("\n") + 1, 1):
                problems.append(f"{len(lines)} lignes au lieu de {src.count(chr(10)) + 1}")
            if JAPANESE.search(fr):
                problems.append("japonais restant")
            if CODE.search(src):
                problems.append("codes de touches à replacer")
            if problems:
                rep.write(f"{escape(src)}\t{escape(fr)}\t{', '.join(problems)}\n")
            done += 1
            if done % 50 == 0:
                out.flush()
                rep.flush()
                rate = done / max(time.time() - started, 1e-6)
                left = (len(todo) - done) / max(rate, 1e-6)
                print(f"  {done}/{len(todo)} — {rate:.1f} textes/s — reste environ {left / 60:.0f} min", flush=True)
    print(f"Terminé : {done} texte(s) en {(time.time() - started) / 60:.1f} min → {args.sortie}")
    print(f"Textes à revoir : {report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
