# Compile Erupe v9.4.1 avec les correctifs HomeFrontier (Build/patches/*.patch) et l'installe dans
# <Root>/Server/erupe/erupe-ce.exe. Le binaire officiel est gardé une fois sous erupe-ce-officiel.exe.
#   powershell -ExecutionPolicy Bypass -File Build\Build-Erupe.ps1 [-Root <dossier d'installation>]
# Go (archive officielle go.dev, empreinte du manifeste) est décompressé dans Build/go, sans installation système.
param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot)
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot 'manifest.json') | ConvertFrom-Json
$cache = Join-Path $PSScriptRoot 'downloads'
New-Item -ItemType Directory -Force $cache | Out-Null
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'

# Go
$goExe = Join-Path $PSScriptRoot 'go\bin\go.exe'
if (-not (Test-Path $goExe)) {
    $go = $manifest.components.go
    $zip = Join-Path $cache ([IO.Path]::GetFileName($go.url))
    if (-not (Test-Path $zip)) {
        Write-Host "Téléchargement de $($go.url)"
        & curl.exe -sSL -o $zip $go.url
        if ($LASTEXITCODE -ne 0) { throw "échec du téléchargement : $($go.url)" }
    }
    $hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
    if ($hash -ne $go.sha256) { Remove-Item $zip; throw "empreinte inattendue pour $zip : $hash" }
    & $tar -xf $zip -C $PSScriptRoot
    if ($LASTEXITCODE -ne 0) { throw 'échec de l''extraction de Go' }
}

# Sources d'Erupe au commit du manifeste, correctifs appliqués.
$erupe = $manifest.components.erupe
$src = Join-Path $PSScriptRoot 'erupe-src'
if (-not (Test-Path (Join-Path $src '.git'))) {
    & git clone --quiet $erupe.repo $src
    if ($LASTEXITCODE -ne 0) { throw 'échec du clonage d''Erupe' }
}
& git -C $src checkout --quiet --force $erupe.commit
if ($LASTEXITCODE -ne 0) { throw "commit $($erupe.commit) introuvable" }
foreach ($patch in Get-ChildItem (Join-Path $PSScriptRoot 'patches') -Filter 'erupe-*.patch') {
    & git -C $src apply --whitespace=nowarn $patch.FullName
    if ($LASTEXITCODE -ne 0) { throw "correctif non applicable : $($patch.Name)" }
    Write-Host "Correctif appliqué : $($patch.Name)"
}

# Compilation (comme la release officielle : go build), tests du paquet modifié d'abord.
$env:GOTOOLCHAIN = 'local'
Push-Location $src
try {
    & $goExe test ./common/stringsupport/
    if ($LASTEXITCODE -ne 0) { throw 'tests en échec' }
    & $goExe build -o erupe-ce.exe
    if ($LASTEXITCODE -ne 0) { throw 'échec de la compilation' }
} finally { Pop-Location }

$dest = Join-Path $Root 'Server\erupe'
New-Item -ItemType Directory -Force $dest | Out-Null
$official = Join-Path $dest 'erupe-ce-officiel.exe'
$target = Join-Path $dest 'erupe-ce.exe'
if ((Test-Path $target) -and -not (Test-Path $official)) { Copy-Item $target $official }
Copy-Item (Join-Path $src 'erupe-ce.exe') $target -Force
Write-Host "Erupe $($erupe.tag) + correctifs HomeFrontier installé dans $dest"
