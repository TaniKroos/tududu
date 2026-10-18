<#
.SYNOPSIS
    Sets up Tududu for development on Windows and runs it.

.DESCRIPTION
    Installs any missing build tools with winget (Git, Rust, Bun, CMake, the
    Vulkan SDK and the Visual Studio C++ Build Tools), clones the repository if
    this isn't already a checkout, installs the JavaScript dependencies,
    downloads the voice-detection model the build needs, then starts Tududu.

    Safe to run again: anything already installed is skipped.

.PARAMETER Build
    Build installers (.msi and -setup.exe) instead of starting the app.

.PARAMETER NoRun
    Set everything up but don't start or build the app.

.PARAMETER Dir
    Where to clone Tududu when the script isn't run from inside a checkout.
    Defaults to a "tududu" folder in your user folder (C:\Users\<you>\tududu),
    whatever folder PowerShell is in. An admin PowerShell starts in
    C:\Windows\System32, which is no place for a clone.

.EXAMPLE
    # From inside a clone:
    powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1

.EXAMPLE
    # Without cloning first:
    powershell -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/TaniKroos/tududu/main/scripts/setup-windows.ps1)))"
#>
[CmdletBinding()]
param(
    [switch]$Build,
    [switch]$NoRun,
    [string]$Dir = ""
)

$ErrorActionPreference = "Stop"
$RepoUrl = "https://github.com/TaniKroos/tududu.git"
$VadUrl = "https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf"

function Write-Step([string]$Message) { Write-Host ""; Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-Ok([string]$Message) { Write-Host "    $Message" -ForegroundColor Green }
function Write-Note([string]$Message) { Write-Host "    $Message" -ForegroundColor Yellow }
function Stop-Setup([string]$Message) { Write-Host ""; Write-Host "Setup stopped: $Message" -ForegroundColor Red; exit 1 }

# winget installs update the system PATH, but not this session's copy of it.
function Update-SessionEnvironment {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $env:Path = "$machine;$user"
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    $bunBin = Join-Path $env:USERPROFILE ".bun\bin"
    foreach ($p in @($cargoBin, $bunBin)) {
        if ((Test-Path $p) -and -not ($env:Path -split ";" | Where-Object { $_ -eq $p })) { $env:Path = "$p;$env:Path" }
    }
    $vulkan = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
    if (-not $vulkan) { $vulkan = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "User") }
    if ($vulkan) { $env:VULKAN_SDK = $vulkan }
}

function Test-Command([string]$Name) { [bool](Get-Command $Name -ErrorAction SilentlyContinue) }

function Install-WithWinget([string]$Id, [string]$Label, [string[]]$Extra = @()) {
    Write-Note "Installing $Label (a Windows prompt may ask for permission)..."
    $wingetArgs = @("install", "--id", $Id, "--exact", "--silent", "--accept-source-agreements", "--accept-package-agreements") + $Extra
    & winget @wingetArgs
    # -1978335189 (0x8A15002B): already installed / no newer version.
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) {
        Stop-Setup "winget couldn't install $Label (exit code $LASTEXITCODE). Install it manually, then run this script again."
    }
    Update-SessionEnvironment
}

function Test-VCTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) { return $false }
    $path = & $vswhere -products * -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$path
}

Write-Host "Tududu setup for Windows" -ForegroundColor Cyan

# --- Checks ---------------------------------------------------------------
if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    Stop-Setup ("building from source on Windows on ARM needs a separate clang-cl toolchain. " +
        "Install the prebuilt ARM64 installer instead: see 'Option A' in SETUP.md.")
}
if (-not (Test-Command "winget")) {
    Stop-Setup "winget isn't available. Install 'App Installer' from the Microsoft Store, then run this script again."
}
Update-SessionEnvironment

# --- Tools ----------------------------------------------------------------
Write-Step "Checking build tools"

if (Test-Command "git") { Write-Ok "Git found" } else { Install-WithWinget "Git.Git" "Git" }

if (Test-VCTools) { Write-Ok "Visual Studio C++ Build Tools found" }
else {
    Install-WithWinget "Microsoft.VisualStudio.2022.BuildTools" "Visual Studio C++ Build Tools (large download, can take 10+ minutes)" @(
        "--override", "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
    if (-not (Test-VCTools)) { Stop-Setup "the C++ Build Tools didn't finish installing. Restart Windows and run this script again." }
}

if (Test-Command "rustup") { Write-Ok "Rust found" } else { Install-WithWinget "Rustlang.Rustup" "Rust" }
if (-not (Test-Command "rustup")) { Stop-Setup "Rust was installed but isn't on PATH yet. Open a new PowerShell window and run this script again." }
& rustup toolchain install stable --profile minimal | Out-Null
if ($LASTEXITCODE -ne 0) { Stop-Setup "rustup couldn't install the stable Rust toolchain." }
& rustup default stable | Out-Null

if (Test-Command "bun") { Write-Ok "Bun found" } else { Install-WithWinget "Oven-sh.Bun" "Bun" }
if (Test-Command "cmake") { Write-Ok "CMake found" } else { Install-WithWinget "Kitware.CMake" "CMake" }
if ($env:VULKAN_SDK -and (Test-Path $env:VULKAN_SDK)) { Write-Ok "Vulkan SDK found" } else { Install-WithWinget "KhronosGroup.VulkanSDK" "Vulkan SDK" }

$missing = @("git", "cargo", "bun", "cmake") | Where-Object { -not (Test-Command $_) }
if ($missing) { Stop-Setup ("these tools were installed but aren't on PATH yet: " + ($missing -join ", ") + ". Open a new PowerShell window and run this script again.") }
if (-not $env:VULKAN_SDK) { Stop-Setup "the Vulkan SDK was installed but VULKAN_SDK isn't set yet. Open a new PowerShell window and run this script again." }

# --- Source ---------------------------------------------------------------
Write-Step "Getting the Tududu source"
$inCheckout = (Test-Path "package.json") -and (Test-Path "src-tauri\tauri.conf.json")
if (-not $inCheckout -and $PSScriptRoot) {
    # Run as scripts\setup-windows.ps1 from somewhere else: use that checkout.
    $root = Split-Path $PSScriptRoot -Parent
    if ((Test-Path (Join-Path $root "package.json")) -and (Test-Path (Join-Path $root "src-tauri\tauri.conf.json"))) {
        Set-Location $root
        $inCheckout = $true
    }
}
if ($inCheckout) {
    Write-Ok "Using the checkout in $((Get-Location).Path)"
} else {
    if (-not $Dir) { $Dir = Join-Path $env:USERPROFILE "tududu" }
    $isCheckout = (Test-Path (Join-Path $Dir "package.json")) -and (Test-Path (Join-Path $Dir "src-tauri\tauri.conf.json"))
    if ($isCheckout) {
        # A previous run already cloned it: reuse that copy.
        Set-Location $Dir
        Write-Ok "Using the checkout in $((Get-Location).Path)"
    } else {
        if (Test-Path $Dir) {
            Stop-Setup "'$Dir' already exists but isn't a Tududu checkout. Pass -Dir with another folder."
        }
        & git clone $RepoUrl $Dir
        if ($LASTEXITCODE -ne 0) { Stop-Setup "git couldn't clone $RepoUrl." }
        Set-Location $Dir
        Write-Ok "Cloned into $((Get-Location).Path)"
    }
}

# --- Dependencies ---------------------------------------------------------
Write-Step "Installing JavaScript dependencies"
& bun install
if ($LASTEXITCODE -ne 0) { Stop-Setup "bun install failed." }

Write-Step "Downloading the voice-detection model"
$modelsDir = "src-tauri\resources\models"
$vadPath = Join-Path $modelsDir "silero-vad-v6.2-F32.gguf"
if (Test-Path $vadPath) { Write-Ok "Already downloaded" }
else {
    New-Item -ItemType Directory -Force -Path $modelsDir | Out-Null
    $ProgressPreference = "SilentlyContinue" # makes Invoke-WebRequest much faster
    Invoke-WebRequest -Uri $VadUrl -OutFile $vadPath
    Write-Ok "Saved to $vadPath"
}

# --- Run ------------------------------------------------------------------
# CMake 4 refuses some dependencies' old minimum-version declarations without this.
$env:CMAKE_POLICY_VERSION_MINIMUM = "3.5"

if ($NoRun) {
    Write-Step "Setup complete"
    Write-Host "    Start Tududu with:"
    Write-Host "      `$env:CMAKE_POLICY_VERSION_MINIMUM = `"3.5`"; bun run tauri dev"
    exit 0
}

if ($Build) {
    Write-Step "Building installers (the first build takes 10-25 minutes)"
    & bun run tauri build
    if ($LASTEXITCODE -ne 0) { Stop-Setup "the build failed. See the Troubleshooting section of SETUP.md." }
    Write-Step "Done. Installers are in src-tauri\target\release\bundle\"
    Get-ChildItem "src-tauri\target\release\bundle" -Recurse -Include *.msi, *-setup.exe | ForEach-Object { Write-Ok $_.FullName }
    exit 0
}

Write-Step "Starting Tududu (the first build takes 10-25 minutes; keep this window open)"
Write-Host "    When the window opens: Models -> download and select 'Canary 1B v2',"
Write-Host "    then General -> Translation -> pick a language. Hold Ctrl+Alt+T and speak English."
& bun run tauri dev
