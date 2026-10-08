<#
.SYNOPSIS
    Prepares a GammaEngine test server from the latest build.

.DESCRIPTION
    Copies the newest jar of build/distributions into the target folder, unpacks libraries.zip
    with the MD5 files the server checks at startup (so the first start downloads nothing), and
    writes the test settings of tools/test-server/config where no file exists yet.

    Run ./gradlew buildPackages first. Running setup again updates the jar and the libraries and
    keeps the world and any setting already changed by hand.

.PARAMETER Target
    Server folder. Default: test-server/ at the repository root, ignored by git.

.PARAMETER AcceptEula
    Writes eula=true. Only pass it if you accept the Minecraft EULA: https://aka.ms/MinecraftEULA

.PARAMETER ResetConfig
    Overwrites the settings with the templates of tools/test-server/config.

.PARAMETER ResetWorld
    Deletes the world folders so the next start generates the world again from the fixed seed.
#>
param(
    [string]$Target = (Join-Path $PSScriptRoot '..\..\test-server'),
    [switch]$AcceptEula,
    [switch]$ResetConfig,
    [switch]$ResetWorld
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$dist = Join-Path $repo 'build\distributions'

$jar = Get-ChildItem -Path $dist -Filter '*-server.jar' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $jar) {
    throw "No server jar in $dist. Run ./gradlew buildPackages first."
}
$libraryArchive = Join-Path $dist 'libraries.zip'
if (-not (Test-Path $libraryArchive)) {
    throw "libraries.zip is missing from $dist. Run ./gradlew buildPackages first."
}

New-Item -ItemType Directory -Force -Path $Target | Out-Null
$Target = (Resolve-Path $Target).Path

Copy-Item -Path $jar.FullName -Destination (Join-Path $Target 'server.jar') -Force
Write-Host "[GammaEngine] Server jar: $($jar.Name)"

# The server refuses a library without a matching .md5 next to it and then downloads it again;
# writing the checksums here keeps the first start offline.
$libraries = Join-Path $Target 'libraries'
Expand-Archive -Path $libraryArchive -DestinationPath $libraries -Force
$count = 0
Get-ChildItem -Path $libraries -Recurse -Filter '*.jar' | ForEach-Object {
    $hash = (Get-FileHash -Path $_.FullName -Algorithm MD5).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText($_.FullName + '.md5', $hash)
    $count++
}
Write-Host "[GammaEngine] $count libraries in place"

$templates = Join-Path $PSScriptRoot 'config'
Get-ChildItem -Path $templates -File | ForEach-Object {
    $destination = Join-Path $Target $_.Name
    if ($ResetConfig -or -not (Test-Path $destination)) {
        Copy-Item -Path $_.FullName -Destination $destination -Force
        Write-Host "[GammaEngine] Settings written: $($_.Name)"
    }
}

$eula = Join-Path $Target 'eula.txt'
if ($AcceptEula) {
    Set-Content -Path $eula -Value 'eula=true' -Encoding ASCII
} elseif (-not ((Test-Path $eula) -and (Select-String -Path $eula -Pattern 'eula=true' -Quiet))) {
    # Test-Path first: under ErrorActionPreference Stop, Select-String on a missing file throws
    # even with -ErrorAction SilentlyContinue.
    Write-Host "[GammaEngine] The Minecraft EULA is not accepted yet: run again with -AcceptEula if you accept it."
}

if ($ResetWorld) {
    foreach ($world in @('world', 'world_nether', 'world_the_end')) {
        $path = Join-Path $Target $world
        if (Test-Path $path) {
            Remove-Item -Path $path -Recurse -Force
            Write-Host "[GammaEngine] World removed: $world"
        }
    }
}

Write-Host "[GammaEngine] Test server ready in $Target"
Write-Host "[GammaEngine] Start it with: tools/test-server/start.ps1"
