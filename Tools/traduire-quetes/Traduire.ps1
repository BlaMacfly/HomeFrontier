# Traduit automatiquement (hors ligne) les textes de quêtes de l'installation HomeFrontier, puis les applique.
#   Traduire.cmd -Racine C:\Jeux\HomeFrontier [-Modele 600M|1.3B] [-Largeur 26] [-Limite 50] [-SansAppliquer]
# Reprend là où il s'est arrêté : relancer après une interruption continue le travail. Les traductions faites à la
# main (Localization\quetes-fr.tsv) ne sont jamais retraduites et restent prioritaires.
param(
    [Parameter(Mandatory = $true)]
    [string]$Racine,
    [ValidateSet('600M', '1.3B')]
    [string]$Modele = '600M',
    [int]$Largeur = 26,
    [int]$Limite = 0,
    [switch]$SansAppliquer
)
$ErrorActionPreference = 'Stop'
$ici = $PSScriptRoot
$python = Join-Path $ici '.venv\Scripts\python.exe'
$modeleDir = Join-Path $ici "modele-nllb-$Modele"
if (-not (Test-Path $python) -or -not (Test-Path (Join-Path $modeleDir 'model.bin'))) {
    Write-Host "Traducteur non installé : lancez d'abord Installer.cmd -Modele $Modele" -ForegroundColor Yellow
    exit 1
}
$launcher = Join-Path $Racine 'HomeFrontier.exe'
if (-not (Test-Path $launcher)) { throw "HomeFrontier.exe introuvable dans $Racine" }
if (Get-Process -Name HomeFrontier, mhf-iel-cli -ErrorAction SilentlyContinue) {
    throw 'Fermez le jeu et le launcher HomeFrontier avant de traduire les quêtes.'
}

$loc = Join-Path $Racine 'Localization'
New-Item -ItemType Directory -Force $loc | Out-Null
$source = Join-Path $loc 'quetes-source.tsv'
if (-not (Test-Path $source)) {
    Write-Host 'Inventaire des textes de quêtes…'
    Start-Process -FilePath $launcher -ArgumentList '--do', 'quest-inventory', "`"$source`"", '--quit-after', '120' `
        -WorkingDirectory $Racine -Wait
    if (-not (Test-Path $source)) { throw "inventaire non produit (voir $Racine\Logs\launcher.log)" }
}

$arguments = @(
    (Join-Path $ici 'traduire_quetes.py'),
    '--source', $source,
    '--manuel', (Join-Path $loc 'quetes-fr.tsv'),
    '--sortie', (Join-Path $loc 'quetes-fr-auto.tsv'),
    '--modele', $modeleDir,
    '--largeur', $Largeur
)
if ($Limite -gt 0) { $arguments += @('--limite', $Limite) }
$env:PYTHONIOENCODING = 'utf-8'
& $python @arguments
if ($LASTEXITCODE -ne 0) { throw 'traduction interrompue' }

if (-not $SansAppliquer) {
    Write-Host 'Application aux quêtes (quelques minutes)…'
    Start-Process -FilePath $launcher -ArgumentList '--do', 'quest-translate', '--quit-after', '5' -WorkingDirectory $Racine -Wait
    Get-Content (Join-Path $Racine 'Logs\launcher.log') -Encoding UTF8 -Tail 3
    Write-Host 'Redémarrez le serveur dans le launcher pour que les quêtes traduites soient chargées.' -ForegroundColor Green
}
