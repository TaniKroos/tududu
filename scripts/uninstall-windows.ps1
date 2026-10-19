<#
.SYNOPSIS
    Removes Tududu and, if you want, the build tools setup-windows.ps1 installed.

.DESCRIPTION
    Goes through everything the Tududu setup put on this PC and asks before
    removing each item:

      Tududu itself (removed by default)
        - the cloned source folder(s), including build output
        - an installed Tududu app (.msi / -setup.exe)
        - settings, history and recordings
        - downloaded speech models
        - build leftovers and Windows Defender exclusions

      Build tools (kept by default, since other programs may use them)
        - Git, Rust, Bun, CMake, the Vulkan SDK, Visual Studio C++ Build Tools
        Tools that setup-windows.ps1 itself installed default to "remove".

    Nothing is deleted without a "y" unless you pass -All.

.PARAMETER DryRun
    Only list what would be removed. Nothing is changed.

.PARAMETER All
    Remove everything listed, including the build tools, without asking.

.PARAMETER Dir
    An extra Tududu source folder to remove, if you cloned it somewhere
    other than your user folder or C:\Windows\System32.

.EXAMPLE
    # See what would be removed:
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-windows.ps1 -DryRun

.EXAMPLE
    # Remove, answering a question for each item:
    powershell -ExecutionPolicy Bypass -File scripts\uninstall-windows.ps1
#>
[CmdletBinding()]
param(
    [switch]$DryRun,
    [switch]$All,
    [string]$Dir = ""
)

$ErrorActionPreference = "Stop"
$Identifiers = @("com.smudgee.tududu", "com.tanikroos.tududu")

function Write-Step([string]$Message) { Write-Host ""; Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-Ok([string]$Message) { Write-Host "    $Message" -ForegroundColor Green }
function Write-Note([string]$Message) { Write-Host "    $Message" -ForegroundColor Yellow }
function Write-Skip([string]$Message) { Write-Host "    $Message" -ForegroundColor DarkGray }

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    return (New-Object Security.Principal.WindowsPrincipal $identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Ask a yes/no question. $DefaultYes picks what Enter means.
function Confirm-Item([string]$Question, [bool]$DefaultYes) {
    if ($DryRun) { return $false }
    if ($All) { return $true }
    $hint = if ($DefaultYes) { "[Y/n]" } else { "[y/N]" }
    $answer = Read-Host "    $Question $hint"
    if ([string]::IsNullOrWhiteSpace($answer)) { return $DefaultYes }
    return $answer.Trim().ToLower().StartsWith("y")
}

function Get-FolderSize([string]$Path) {
    try {
        $bytes = (Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
        if (-not $bytes) { return "empty" }
        if ($bytes -ge 1GB) { return "{0:N1} GB" -f ($bytes / 1GB) }
        return "{0:N0} MB" -f ($bytes / 1MB)
    } catch { return "size unknown" }
}

# Remove-Item fails on very long paths (node_modules, cargo build output) and
# would follow directory junctions; rmdir handles both.
function Remove-Folder([string]$Path) {
    & cmd.exe /c "rmdir /s /q `"$Path`"" 2>$null
    if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue }
    if (Test-Path -LiteralPath $Path) { Write-Note "Couldn't remove everything in $Path (a file may be in use). Close Tududu and try again." }
    else { Write-Ok "Removed $Path" }
}

function Remove-FolderIfConfirmed([string]$Path, [string]$What, [bool]$DefaultYes) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    Write-Host "    $What" -ForegroundColor White
    Write-Host "      $Path ($(Get-FolderSize $Path))"
    if ($DryRun) { Write-Skip "would be removed"; return }
    if (Confirm-Item "Remove it?" $DefaultYes) { Remove-Folder $Path } else { Write-Skip "Kept" }
}

function Test-TududuCheckout([string]$Path) {
    $conf = Join-Path $Path "src-tauri\tauri.conf.json"
    if (-not (Test-Path -LiteralPath $conf)) { return $false }
    $text = Get-Content -LiteralPath $conf -Raw
    foreach ($id in $Identifiers) { if ($text -match [regex]::Escape($id)) { return $true } }
    return $false
}

Write-Host "Tududu uninstall for Windows" -ForegroundColor Cyan
if ($DryRun) { Write-Note "Dry run: nothing will be changed." }

# Removing tools, System32 folders and Defender exclusions needs admin rights.
if (-not $DryRun -and -not (Test-Admin)) {
    if ($PSCommandPath) {
        Write-Note "Asking for administrator rights (needed to remove tools and system folders)..."
        $argList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-NoExit", "-File", "`"$PSCommandPath`"")
        if ($All) { $argList += "-All" }
        if ($Dir) { $argList += @("-Dir", "`"$Dir`"") }
        Start-Process powershell.exe -Verb RunAs -ArgumentList $argList
        exit 0
    }
    Write-Note "Not running as administrator: some items may fail to remove. For a full cleanup, run this from an admin PowerShell."
}

# Don't delete the folder we're standing in.
Set-Location $env:USERPROFILE

# --- Stop Tududu --------------------------------------------------------------
Write-Step "Stopping Tududu if it's running"
# Dev builds run as handy.exe, like the real Handy app: match on the path.
$procs = @(Get-Process -Name "handy", "Tududu" -ErrorAction SilentlyContinue | Where-Object { $_.Path -match "tududu" })
if ($procs.Count -eq 0) { Write-Skip "Not running" }
elseif ($DryRun) { Write-Skip "Would stop $($procs.Count) process(es)" }
else { $procs | Stop-Process -Force; Write-Ok "Stopped" }

# --- Installed app ------------------------------------------------------------
Write-Step "Installed Tududu app"
$uninstallKeys = @(
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
)
$apps = @(Get-ItemProperty $uninstallKeys -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq "Tududu" })
if ($apps.Count -eq 0) { Write-Skip "Not installed" }
foreach ($app in $apps) {
    Write-Host "    Tududu $($app.DisplayVersion) (installed app)" -ForegroundColor White
    if ($DryRun) { Write-Skip "would be uninstalled"; continue }
    if (-not (Confirm-Item "Uninstall it?" $true)) { Write-Skip "Kept"; continue }
    if ($app.PSChildName -match '^\{[0-9A-Fa-f-]+\}$') {
        Start-Process msiexec.exe -ArgumentList "/x $($app.PSChildName) /qn" -Wait
    } else {
        $cmd = if ($app.QuietUninstallString) { $app.QuietUninstallString } else { "$($app.UninstallString) /S" }
        Start-Process cmd.exe -ArgumentList "/c $cmd" -Wait
    }
    Write-Ok "Uninstalled"
}

# --- Source folders -----------------------------------------------------------
Write-Step "Tududu source folders (code, dependencies and build output)"
$candidates = @(
    (Join-Path $env:USERPROFILE "tududu"),
    (Join-Path $env:WINDIR "System32\tududu")
)
if ($Dir) { $candidates = @($Dir) + $candidates }
if ($PSScriptRoot) { $candidates += (Split-Path $PSScriptRoot -Parent) }
$found = @($candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | ForEach-Object { (Resolve-Path -LiteralPath $_).Path } | Select-Object -Unique)
$checkouts = @($found | Where-Object { Test-TududuCheckout $_ })
if ($checkouts.Count -eq 0) { Write-Skip "None found (pass -Dir if you cloned it somewhere else)" }
foreach ($path in $checkouts) { Remove-FolderIfConfirmed $path "Tududu source folder" $true }

# --- Settings, history, recordings ------------------------------------------
Write-Step "Settings, history and recordings"
$dataFound = $false
foreach ($id in $Identifiers) {
    foreach ($base in @($env:APPDATA, $env:LOCALAPPDATA)) {
        $path = Join-Path $base $id
        if (Test-Path -LiteralPath $path) {
            $dataFound = $true
            Remove-FolderIfConfirmed $path "Tududu data (your transcription history and recordings are here)" $true
        }
    }
}
if (-not $dataFound) { Write-Skip "None found" }

# --- Models -------------------------------------------------------------------
Write-Step "Downloaded speech and text models"
$hfHome = if ($env:HF_HOME) { $env:HF_HOME } else { Join-Path $env:USERPROFILE ".cache\huggingface" }
$hub = Join-Path $hfHome "hub"
# Speech models come from handy-computer; text models are listed in
# src-tauri/src/managers/text_model.rs (TEXT_MODELS).
$textModelRepos = @("models--ggml-org--gemma-3-1b-it-GGUF")
$models = @()
if (Test-Path -LiteralPath $hub) {
    $models = @(Get-ChildItem -LiteralPath $hub -Directory | Where-Object {
            $_.Name -like "models--handy-computer--*" -or $textModelRepos -contains $_.Name
        })
}
if ($models.Count -eq 0) { Write-Skip "None found" }
else {
    Write-Note "Speech models are shared with the Handy app, if you also use it."
    foreach ($m in $models) {
        $name = $m.Name -replace '^models--.*?--', ''
        Remove-FolderIfConfirmed $m.FullName "Model: $name" $true
    }
}

# --- Build leftovers ----------------------------------------------------------
Write-Step "Build leftovers"
# transcribe-cpp compiles through short junctions here to dodge Windows' path limit.
Remove-FolderIfConfirmed (Join-Path $env:LOCALAPPDATA "tcs") "Speech engine build shortcuts" $true
Remove-FolderIfConfirmed (Join-Path $env:LOCALAPPDATA "Tududu-setup") "Setup script records" $true

if (Get-Command Get-MpPreference -ErrorAction SilentlyContinue) {
    $exclusions = @()
    try { $exclusions = @((Get-MpPreference).ExclusionPath | Where-Object { $_ -match "tududu" }) }
    catch { Write-Skip "Couldn't read Windows Defender settings; skipping exclusions." }
    foreach ($ex in $exclusions) {
        Write-Host "    Windows Defender exclusion: $ex" -ForegroundColor White
        if ($DryRun) { Write-Skip "would be removed"; continue }
        if (Confirm-Item "Remove it?" $true) {
            try { Remove-MpPreference -ExclusionPath $ex; Write-Ok "Removed" } catch { Write-Note "Couldn't remove it (needs administrator)." }
        } else { Write-Skip "Kept" }
    }
}

# --- Build tools ---------------------------------------------------------------
Write-Step "Build tools"
Write-Note "Other programs may use these. Keep any you had before Tududu."
$stateFile = Join-Path $env:LOCALAPPDATA "Tududu-setup\installed-tools.txt"
$installedBySetup = @()
if (Test-Path $stateFile) { $installedBySetup = @(Get-Content $stateFile) }

$tools = @(
    @{ Id = "Microsoft.VisualStudio.2022.BuildTools"; Label = "Visual Studio C++ Build Tools (several GB)" },
    @{ Id = "KhronosGroup.VulkanSDK"; Label = "Vulkan SDK" },
    @{ Id = "Kitware.CMake"; Label = "CMake" },
    @{ Id = "Oven-sh.Bun"; Label = "Bun" },
    @{ Id = "Rustlang.Rustup"; Label = "Rust (rustup, plus its .cargo and .rustup folders)" },
    @{ Id = "Git.Git"; Label = "Git" }
)
$hasWinget = [bool](Get-Command winget -ErrorAction SilentlyContinue)
if (-not $hasWinget) { Write-Skip "winget isn't available; remove the tools from Settings > Apps instead." }
foreach ($tool in $tools) {
    if (-not $hasWinget) { break }
    & winget list --id $tool.Id --exact --accept-source-agreements *> $null
    if ($LASTEXITCODE -ne 0) { continue }
    $bySetup = $installedBySetup -contains $tool.Id
    $origin = if ($bySetup) { "installed by Tududu setup" } else { "may have been installed before Tududu" }
    Write-Host "    $($tool.Label) ($origin)" -ForegroundColor White
    if ($DryRun) { Write-Skip $(if ($bySetup -or $All) { "would be removed" } else { "would be kept unless you answer y" }); continue }
    if (-not (Confirm-Item "Uninstall it?" $bySetup)) { Write-Skip "Kept"; continue }

    if ($tool.Id -eq "Rustlang.Rustup" -and (Get-Command rustup -ErrorAction SilentlyContinue)) {
        # Removes the toolchains and the .cargo / .rustup folders too.
        & rustup self uninstall -y
    }
    & winget uninstall --id $tool.Id --exact --silent --accept-source-agreements
    if ($tool.Id -eq "Oven-sh.Bun") {
        $bunDir = Join-Path $env:USERPROFILE ".bun"
        if (Test-Path -LiteralPath $bunDir) { Remove-Folder $bunDir }
    }
    Write-Ok "Uninstalled $($tool.Label)"
}

Write-Step $(if ($DryRun) { "Dry run finished. Run without -DryRun to remove." } else { "Done" })
if (-not $DryRun) { Write-Host "    Restart PowerShell so removed tools disappear from PATH." }
