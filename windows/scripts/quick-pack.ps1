# Quick test pack: build both exes --release (no fmt/test/clippy gates) and
# stage them under the project temp folder (windows\tmp\quick-test) with the
# same fixed names every time, then restart the resident and manager from
# there. Use package-release.ps1 for a fully gated, versioned rNN release.
#
# Examples:
#   windows\scripts\quick-pack.ps1            # build + stage + restart
#   windows\scripts\quick-pack.ps1 -SkipBuild # restart from last built exes
[CmdletBinding()]
param(
    # Skip the cargo build and restart from the last binaries.
    [switch] $SkipBuild,
    # Stage and restart only the resident (default builds both apps).
    [switch] $ResidentOnly
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$buildTemp = Join-Path $repoRoot "windows\tmp"
$outputDirectory = Join-Path $buildTemp "quick-test"
$targetDirectory = Join-Path $repoRoot "windows\target"
$releaseDirectory = Join-Path $targetDirectory "release"
$configTemplate = Join-Path $repoRoot "windows\config\config.example.toml"
$cargo = "D:\DevTools\cargo\bin\cargo.exe"
$vsDevCmd = "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat"
$rustupHome = "D:\DevTools\rustup"
$cargoHome = "D:\DevTools\cargo"
$residentExe = "selection-translate-resident.exe"
$managerExe = "selection-translate-manager.exe"

foreach ($requiredPath in @($cargo, $vsDevCmd)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Required tool was not found: $requiredPath"
    }
}

$env:RUSTUP_HOME = $rustupHome
$env:CARGO_HOME = $cargoHome
$env:CARGO_TARGET_DIR = $targetDirectory
$env:TEMP = $buildTemp
$env:TMP = $buildTemp

function Invoke-VsCargo {
    param([Parameter(Mandatory = $true)][string[]] $CargoArguments)
    $quotedArguments = $CargoArguments | ForEach-Object {
        '"' + ($_ -replace '"', '\"') + '"'
    }
    $commandLine = 'call "{0}" -arch=x64 && "{1}" {2}' -f $vsDevCmd, $cargo, ($quotedArguments -join " ")
    Push-Location $repoRoot
    try {
        & $env:ComSpec /d /s /c $commandLine
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo command failed with exit code $($LASTEXITCODE): cargo $($CargoArguments -join ' ')"
        }
    } finally {
        Pop-Location
    }
}

function Stop-TranslateProcess {
    param([Parameter(Mandatory = $true)][string] $Name)
    foreach ($process in @(Get-Process -Name $Name -ErrorAction SilentlyContinue)) {
        Write-Host "Stopping $($process.ProcessName) pid=$($process.Id)"
        Stop-Process -Id $process.Id -Force
    }
}

if (-not $SkipBuild) {
    $packages = @("-p", "selection-translate-resident")
    if (-not $ResidentOnly) {
        $packages += @("-p", "selection-translate-manager")
    }
    Write-Host "Quick build: cargo build --locked --release $($packages -join ' ') (no gates)"
    Invoke-VsCargo -CargoArguments (@("build", "--locked", "--release") + $packages)
}

if (-not (Test-Path -LiteralPath (Join-Path $releaseDirectory $residentExe) -PathType Leaf)) {
    throw "Built binary missing: $(Join-Path $releaseDirectory $residentExe)"
}

# Same fixed folder every time: the quick test build is scratch, not history.
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
foreach ($process in @(Get-Process -Name "selection-translate-*" -ErrorAction SilentlyContinue)) {
    Write-Host "Stopping $($process.ProcessName) pid=$($process.Id)"
    Stop-Process -Id $process.Id -Force
}
Start-Sleep -Milliseconds 500
Copy-Item -LiteralPath (Join-Path $releaseDirectory $residentExe) -Destination (Join-Path $outputDirectory $residentExe) -Force
if (-not $ResidentOnly -and (Test-Path -LiteralPath (Join-Path $releaseDirectory $managerExe) -PathType Leaf)) {
    Copy-Item -LiteralPath (Join-Path $releaseDirectory $managerExe) -Destination (Join-Path $outputDirectory $managerExe) -Force
}

Start-Process -FilePath (Join-Path $outputDirectory $residentExe) -WorkingDirectory $outputDirectory
if (-not $ResidentOnly) {
    Start-Process -FilePath (Join-Path $outputDirectory $managerExe) -WorkingDirectory $outputDirectory
}
Write-Host ""
Write-Host "Quick test build staged and running from: $outputDirectory"
Write-Host "Selection/hover translation is live - verify your changes, then pack a release with package-release.ps1 when satisfied."
