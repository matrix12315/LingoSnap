# Fast edit loop: rebuild resident after popup.rs changes and restart it.
# Final binaries are copied to the project temp folder (windows\tmp).
# Skips fmt/test/clippy/package gates — use package-release.ps1 for a full package.
[CmdletBinding()]
param(
    # Also rebuild the manager (shares platform-windows / popup.rs).
    [switch] $WithManager,
    # Restart without building (use last cargo target binaries, still copy to temp).
    [switch] $RestartOnly,
    # Where to place runnable exes. Default: windows\tmp (project temp).
    [string] $BuildOutput,
    # Keep a currently running resident instead of restarting it.
    [switch] $KeepRunning
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
if ([string]::IsNullOrWhiteSpace($BuildOutput)) {
    $BuildOutput = Join-Path $repoRoot "windows\tmp"
} elseif (-not [IO.Path]::IsPathRooted($BuildOutput)) {
    $BuildOutput = [IO.Path]::GetFullPath((Join-Path $repoRoot $BuildOutput))
} else {
    $BuildOutput = [IO.Path]::GetFullPath($BuildOutput)
}
$cargo = "D:\DevTools\cargo\bin\cargo.exe"
$vsDevCmd = "D:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat"
$rustupHome = "D:\DevTools\rustup"
$cargoHome = "D:\DevTools\cargo"
$buildTemp = Join-Path $repoRoot "windows\tmp"
$targetDirectory = Join-Path $repoRoot "windows\target"
$releaseDirectory = Join-Path $targetDirectory "release"
$residentExe = Join-Path $releaseDirectory "selection-translate-resident.exe"
$managerExe = Join-Path $releaseDirectory "selection-translate-manager.exe"

foreach ($requiredPath in @($cargo, $vsDevCmd)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Required tool was not found: $requiredPath"
    }
}

New-Item -ItemType Directory -Path $buildTemp -Force | Out-Null
New-Item -ItemType Directory -Path $BuildOutput -Force | Out-Null
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
    $processes = @(Get-Process -Name $Name -ErrorAction SilentlyContinue)
    foreach ($process in $processes) {
        Write-Host "Stopping $($process.ProcessName) pid=$($process.Id)"
        Stop-Process -Id $process.Id -Force
    }
    if ($processes.Count -gt 0) {
        Start-Sleep -Milliseconds 400
    }
}

if (-not $RestartOnly) {
    $packages = @("-p", "selection-translate-resident")
    if ($WithManager) {
        $packages += @("-p", "selection-translate-manager")
    }
    Write-Host "Building $($packages -join ' ') --release --locked"
    Invoke-VsCargo -CargoArguments (@("build", "--locked", "--release") + $packages)
}

if (-not (Test-Path -LiteralPath $residentExe -PathType Leaf)) {
    throw "Resident binary missing after build: $residentExe"
}

if (-not $KeepRunning) {
    Stop-TranslateProcess -Name "selection-translate-resident"
}

# Always stage runnable binaries in the temp output folder.
Copy-Item -LiteralPath $residentExe -Destination (Join-Path $BuildOutput "selection-translate-resident.exe") -Force
if ($WithManager -and (Test-Path -LiteralPath $managerExe -PathType Leaf)) {
    Copy-Item -LiteralPath $managerExe -Destination (Join-Path $BuildOutput "selection-translate-manager.exe") -Force
}
Write-Host "Build saved to: $BuildOutput"

$launchPath = Join-Path $BuildOutput "selection-translate-resident.exe"
if (-not $KeepRunning) {
    Start-Process -FilePath $launchPath
    Write-Host "Started resident: $launchPath"
    Write-Host "Drag-select text (>= 6px) to show the real profile bar."
} else {
    Write-Host "Build finished. Running processes were left untouched."
    Write-Host "Binary: $launchPath"
}
