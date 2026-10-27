# Télécharge les composants tiers figés dans Build/manifest.json, vérifie leur SHA-256 et les installe sous
# <Root>/Server (erupe, pgsql). Les archives restent en cache dans Build/downloads.
#   powershell -ExecutionPolicy Bypass -File Build\Get-Components.ps1 [-Root <dossier d'installation>]
param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot)
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot 'manifest.json') | ConvertFrom-Json
$cache = Join-Path $PSScriptRoot 'downloads'
New-Item -ItemType Directory -Force $cache | Out-Null

function Get-Verified($component) {
    $file = Join-Path $cache ([IO.Path]::GetFileName($component.url))
    if (-not (Test-Path $file)) {
        Write-Host "Téléchargement de $($component.url)"
        & curl.exe -sSL -o $file $component.url
        if ($LASTEXITCODE -ne 0) { throw "échec du téléchargement : $($component.url)" }
    }
    $hash = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLowerInvariant()
    if ($hash -ne $component.sha256) {
        Remove-Item $file
        throw "empreinte inattendue pour $file : $hash (attendu $($component.sha256))"
    }
    return $file
}

$tar = Join-Path $env:SystemRoot 'System32\tar.exe'

# Erupe : erupe-ce.exe + configurations d'exemple, à plat dans Server/erupe.
$erupeZip = Get-Verified $manifest.components.erupe
$erupeDir = Join-Path $Root 'Server\erupe'
New-Item -ItemType Directory -Force $erupeDir | Out-Null
& $tar -xf $erupeZip -C $erupeDir
if ($LASTEXITCODE -ne 0) { throw 'échec de l''extraction d''Erupe' }
Write-Host "Erupe $($manifest.components.erupe.tag) installé dans $erupeDir"

# PostgreSQL : seuls bin, lib et share servent (ni pgAdmin, ni StackBuilder, ni documentation).
$pgZip = Get-Verified $manifest.components.postgresql
$serverDir = Join-Path $Root 'Server'
& $tar -xf $pgZip -C $serverDir 'pgsql/bin' 'pgsql/lib' 'pgsql/share' 'pgsql/server_license.txt'
if ($LASTEXITCODE -ne 0) { throw 'échec de l''extraction de PostgreSQL' }
Write-Host "PostgreSQL $($manifest.components.postgresql.version) installé dans $(Join-Path $serverDir 'pgsql')"

# Traductions françaises du client (GPL-3.0) : décompressées à part, lues par le launcher (onglet Français).
$trGz = Get-Verified $manifest.components.'translation-fr'
$locDir = Join-Path $Root 'Localization'
New-Item -ItemType Directory -Force $locDir | Out-Null
$in = [IO.File]::OpenRead($trGz)
try {
    $gz = New-Object IO.Compression.GZipStream($in, [IO.Compression.CompressionMode]::Decompress)
    $out = [IO.File]::Create((Join-Path $locDir 'translations-fr.json'))
    try { $gz.CopyTo($out) } finally { $out.Dispose(); $gz.Dispose() }
} finally { $in.Dispose() }
Write-Host "Traductions $($manifest.components.'translation-fr'.tag) installées dans $locDir"

# Traductions des quêtes du serveur (versionnées dans ce dépôt).
$questTsv = Join-Path (Split-Path -Parent $PSScriptRoot) 'Localization\quetes-fr.tsv'
$questDest = Join-Path $locDir 'quetes-fr.tsv'
if ((Test-Path $questTsv) -and ((Resolve-Path $questTsv).Path -ne $questDest)) {
    Copy-Item $questTsv $questDest -Force
    Write-Host "Traductions des quêtes copiées dans $locDir"
}
