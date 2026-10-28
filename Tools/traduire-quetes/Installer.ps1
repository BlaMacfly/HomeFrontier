# Installe le traducteur hors ligne des quêtes : environnement Python privé (.venv), CTranslate2, puis
# téléchargement et conversion du modèle NLLB-200 de Meta (licence CC-BY-NC 4.0 : usage non commercial).
#   Installer.cmd [-Modele 600M|1.3B]
# 600M : environ 2,5 Go à télécharger, 600 Mo une fois converti, 4 Go de mémoire. 1.3B : meilleure qualité,
# environ 5,5 Go à télécharger, 1,4 Go converti, 8 Go de mémoire, deux fois plus lent.
param(
    [ValidateSet('600M', '1.3B')]
    [string]$Modele = '600M'
)
$ErrorActionPreference = 'Stop'
$ici = $PSScriptRoot

# Python 3.10 à 3.12 (le lanceur « py » de python.org ; l'alias du Microsoft Store ne convient pas).
# (jamais « 2>$null » sur un exécutable natif en PowerShell 5.1 : passer par cmd /c … >nul 2>&1)
$py = $null
if (Get-Command py -ErrorAction SilentlyContinue) {
    foreach ($v in '3.12', '3.11', '3.10') {
        cmd /c "py -$v -c ""import sys"" >nul 2>&1"
        if ($LASTEXITCODE -eq 0) { $py = @('py', "-$v"); break }
    }
}
if (-not $py) {
    Write-Host 'Python 3.10 à 3.12 est introuvable. Installez-le puis relancez Installer.cmd :' -ForegroundColor Yellow
    Write-Host '    winget install -e --id Python.Python.3.12'
    Write-Host '(ou https://www.python.org/downloads/ en cochant « Add python.exe to PATH »)'
    exit 1
}
Write-Host "Python : $($py -join ' ')"

$venv = Join-Path $ici '.venv'
$python = Join-Path $venv 'Scripts\python.exe'
if (-not (Test-Path $python)) {
    Write-Host 'Création de l''environnement Python privé (.venv)…'
    & $py[0] $py[1] -m venv $venv
    if ($LASTEXITCODE -ne 0) { throw 'création de .venv impossible' }
}
& $python -m pip install --upgrade pip --quiet
& $python -m pip install -r (Join-Path $ici 'requirements.txt')
if ($LASTEXITCODE -ne 0) { throw 'installation des paquets impossible' }

$nom = if ($Modele -eq '1.3B') { 'facebook/nllb-200-distilled-1.3B' } else { 'facebook/nllb-200-distilled-600M' }
$sortie = Join-Path $ici "modele-nllb-$Modele"
if (Test-Path (Join-Path $sortie 'model.bin')) {
    Write-Host "Modèle déjà installé : $sortie"
    exit 0
}

# PyTorch ne sert qu'à la conversion ; il est retiré ensuite pour libérer de la place.
Write-Host 'Installation temporaire de PyTorch (CPU) pour la conversion…'
& $python -m pip install torch --index-url https://download.pytorch.org/whl/cpu
if ($LASTEXITCODE -ne 0) { throw 'installation de PyTorch impossible' }

Write-Host "Téléchargement et conversion de $nom (plusieurs minutes)…"
& (Join-Path $venv 'Scripts\ct2-transformers-converter.exe') --model $nom --output_dir $sortie --quantization int8 `
    --copy_files tokenizer.json tokenizer_config.json special_tokens_map.json sentencepiece.bpe.model
if ($LASTEXITCODE -ne 0) { throw 'conversion du modèle impossible' }

& $python -m pip uninstall -y torch | Out-Null
Write-Host "Modèle prêt : $sortie" -ForegroundColor Green
Write-Host 'Lancez maintenant Traduire.cmd -Racine <dossier HomeFrontier>.'
