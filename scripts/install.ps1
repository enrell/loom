<#
.SYNOPSIS
    Builds Loom from this checkout and installs it for the current user, with
    Start menu, PATH and Apps & features integration. No admin needed.

.DESCRIPTION
    Installs into %LOCALAPPDATA%\Programs\Loom:
      loom.exe              the CLI (also `loom mcp`)
      loom-desktop.exe      the desktop app (Start menu: "Loom")
      openvfs-inject.exe    hook launch mode (from the sibling openvfs checkout)
      openvfs_hook.dll
    and adds that folder to the user PATH. Running it again rebuilds and
    replaces the binaries, even while Loom is open (the running copy is moved
    aside and cleaned up on the next run).

.EXAMPLE
    .\scripts\install.ps1               # build + install (or reinstall)
    .\scripts\install.ps1 -Update       # git pull loom and openvfs, then rebuild if anything changed
    .\scripts\install.ps1 -Nxm          # also make Loom the nxm:// handler (Nexus "Mod Manager Download")
    .\scripts\install.ps1 -CliOnly      # skip the desktop app (no Node.js needed)
    .\scripts\install.ps1 -Uninstall
#>
[CmdletBinding()]
param(
    [switch]$Update,
    [switch]$Uninstall,
    [switch]$Force,
    [switch]$CliOnly,
    [switch]$Nxm,
    [switch]$DebugBuild,
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\Loom'),
    # Name of the Start menu entry and the Apps & features key (tests use another).
    [string]$Id = 'Loom',
    # Leave PATH, Start menu, Apps & features and nxm:// alone (just build + copy).
    [switch]$NoIntegration
)

$ErrorActionPreference = 'Stop'
$Repo = Split-Path -Parent $PSScriptRoot
$OpenVfs = Join-Path (Split-Path -Parent $Repo) 'openvfs'
$StartMenu = Join-Path ([Environment]::GetFolderPath('Programs')) "$Id.lnk"
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$Id"
$NxmKey = 'HKCU:\Software\Classes\nxm'
$InfoFile = Join-Path $Prefix 'install.json'
$Profile_ = if ($DebugBuild) { 'debug' } else { 'release' }

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Note($msg) { Write-Host "    $msg" }

# Runs a native command and stops on a non-zero exit code.
function Run([string]$exe, [string[]]$argv, [string]$cwd = $Repo) {
    Push-Location $cwd
    try {
        & $exe @argv
        if ($LASTEXITCODE -ne 0) { throw "$exe $($argv -join ' ') failed (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
}

function Need([string]$cmd, [string]$hint) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) { throw "$cmd not found. $hint" }
}

function Commit([string]$dir) {
    $c = git -C $dir rev-parse HEAD 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    $dirty = git -C $dir status --porcelain 2>$null
    if ($dirty) { "$c-dirty" } else { $c }
}

function TargetDir([string]$workspace) {
    if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR $Profile_ } else { Join-Path $workspace "target\$Profile_" }
}

# ---------------------------------------------------------------- PATH

function Broadcast-Environment {
    if (-not ('LoomInstall.Native' -as [type])) {
        Add-Type -Namespace LoomInstall -Name Native -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out System.UIntPtr lpdwResult);
'@
    }
    $out = [UIntPtr]::Zero
    # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG: Explorer picks up the new PATH.
    [void][LoomInstall.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$out)
}

# Edits the user PATH in the registry, keeping REG_EXPAND_SZ entries unexpanded.
function Edit-UserPath([scriptblock]$change) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $old = [string]$key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
        $parts = @($old -split ';' | Where-Object { $_ })
        $new = (& $change $parts) -join ';'
        if ($new -ne $old) {
            $key.SetValue('Path', $new, 'ExpandString')
            Broadcast-Environment
            return $true
        }
        return $false
    } finally { $key.Close() }
}

function Same-Path($a, $b) { $a.TrimEnd('\') -ieq $b.TrimEnd('\') }

# ---------------------------------------------------------------- nxm://

function Get-NxmCommand {
    $k = "$NxmKey\shell\open\command"
    if (Test-Path $k) { (Get-Item $k).GetValue('') } else { $null }
}

function Register-Nxm([string]$exe) {
    $previous = Get-NxmCommand
    $ours = "`"$exe`" `"%1`""
    if ($previous -and $previous -ne $ours) {
        # Remember who handled nxm:// before so -Uninstall can hand it back.
        Set-Content -LiteralPath (Join-Path $Prefix 'nxm-previous.txt') -Value $previous -Encoding UTF8
    }
    New-Item -Path "$NxmKey\shell\open\command" -Force | Out-Null
    Set-ItemProperty -Path $NxmKey -Name '(default)' -Value 'URL:nxm'
    Set-ItemProperty -Path $NxmKey -Name 'URL Protocol' -Value ''
    Set-ItemProperty -Path "$NxmKey\shell\open\command" -Name '(default)' -Value $ours
    Note "nxm:// links now open in Loom"
}

function Unregister-Nxm([string]$exe) {
    $current = Get-NxmCommand
    if (-not $current -or $current -notlike "*$exe*") { return }
    $saved = Join-Path $Prefix 'nxm-previous.txt'
    if (Test-Path $saved) {
        $previous = (Get-Content -LiteralPath $saved -Raw).Trim()
        Set-ItemProperty -Path "$NxmKey\shell\open\command" -Name '(default)' -Value $previous
        Note "nxm:// handed back to: $previous"
    } else {
        Remove-Item -Path $NxmKey -Recurse -Force
        Note 'nxm:// handler removed'
    }
}

# ---------------------------------------------------------------- uninstall

function Uninstall-Loom {
    Step "Uninstalling $Id from $Prefix"
    Unregister-Nxm (Join-Path $Prefix 'loom-desktop.exe')
    if (Test-Path $StartMenu) { Remove-Item -LiteralPath $StartMenu -Force; Note 'Start menu entry removed' }
    if (Test-Path $UninstallKey) { Remove-Item -Path $UninstallKey -Recurse -Force; Note 'Apps & features entry removed' }
    if (Edit-UserPath { param($p) $p | Where-Object { -not (Same-Path $_ $Prefix) } }) { Note 'removed from PATH' }
    $env:Path = (($env:Path -split ';') | Where-Object { $_ -and -not (Same-Path $_ $Prefix) }) -join ';'
    if (Test-Path $Prefix) {
        # This script may be running from $Prefix (Apps & features "Uninstall"):
        # delete what we can now and let a detached cmd finish the folder.
        Get-ChildItem -LiteralPath $Prefix -Force | Where-Object { $_.FullName -ne $PSCommandPath } |
            Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        if ($PSCommandPath -and (Same-Path (Split-Path -Parent $PSCommandPath) $Prefix)) {
            Start-Process cmd.exe -ArgumentList "/c timeout /t 2 >nul & rmdir /s /q `"$Prefix`"" -WindowStyle Hidden
        } else {
            Remove-Item -LiteralPath $Prefix -Recurse -Force -ErrorAction SilentlyContinue
            if (Test-Path $Prefix) { Note "some files are in use; close Loom and delete $Prefix" }
        }
    }
    Note "your mod library (%LOCALAPPDATA%\Loom) was left alone"
    Write-Host "Loom uninstalled." -ForegroundColor Green
}

if ($Uninstall) { Uninstall-Loom; return }

# ---------------------------------------------------------------- sources

Need git 'Install Git: winget install Git.Git'
Need cargo 'Install Rust: winget install Rustlang.Rustup'
if (-not $CliOnly) { Need npm 'Install Node.js (winget install OpenJS.NodeJS.LTS), or pass -CliOnly' }

if (-not (Test-Path (Join-Path $OpenVfs 'Cargo.toml'))) {
    Step "Cloning openvfs next to loom ($OpenVfs)"
    Run git @('clone', 'https://github.com/enrell/openvfs', $OpenVfs)
}

if ($Update) {
    foreach ($dir in @($Repo, $OpenVfs)) {
        Step "Updating $(Split-Path -Leaf $dir)"
        Run git @('pull', '--ff-only') $dir
    }
}

$source = @{ loom = (Commit $Repo); openvfs = (Commit $OpenVfs) }
$installed = if (Test-Path $InfoFile) { Get-Content -LiteralPath $InfoFile -Raw | ConvertFrom-Json } else { $null }
$haveDesktop = Test-Path (Join-Path $Prefix 'loom-desktop.exe')
if ($Update -and -not $Force -and $installed -and
    $installed.loom -eq $source.loom -and $installed.openvfs -eq $source.openvfs -and
    $source.loom -notlike '*-dirty' -and ($CliOnly -or $haveDesktop)) {
    Write-Host "Loom is up to date ($($source.loom.Substring(0, 7)))." -ForegroundColor Green
    return
}

# ---------------------------------------------------------------- build

$cargoProfile = if ($DebugBuild) { @() } else { @('--release') }

Step 'Building loom (CLI)'
Run cargo (@('build', '-p', 'loom-cli') + $cargoProfile)

Step 'Building openvfs hook (openvfs-inject.exe, openvfs_hook.dll)'
Run cargo (@('build', '-p', 'openvfs-hook') + $cargoProfile) $OpenVfs

if (-not $CliOnly) {
    $desktop = Join-Path $Repo 'apps\desktop'
    $stamp = Join-Path $desktop 'node_modules\.package-lock.json'
    if (-not (Test-Path $stamp) -or
        (Get-Item (Join-Path $desktop 'package-lock.json')).LastWriteTime -gt (Get-Item $stamp).LastWriteTime) {
        Step 'Installing desktop app dependencies (npm ci)'
        Run npm @('ci') $desktop
    }
    Step 'Building desktop app (tauri build --no-bundle)'
    $tauriArgs = @('tauri', 'build', '--no-bundle')
    if ($DebugBuild) { $tauriArgs += '--debug' }
    Run npx $tauriArgs $desktop
}

# ---------------------------------------------------------------- install

Step "Installing to $Prefix"
New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
# Copies left aside by an earlier run while Loom was open.
Get-ChildItem -LiteralPath $Prefix -Filter '*.old-*' -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction SilentlyContinue

# Replaces $dest even if it is running: Windows lets a running .exe/.dll be
# renamed, so the old one is moved aside instead of overwritten.
function Install-File([string]$src, [string]$dest) {
    if (-not (Test-Path -LiteralPath $src)) { throw "build output missing: $src" }
    if (Test-Path -LiteralPath $dest) {
        try { Remove-Item -LiteralPath $dest -Force }
        catch {
            Move-Item -LiteralPath $dest -Destination "$dest.old-$([DateTime]::Now.Ticks)" -Force
            Note "$(Split-Path -Leaf $dest) was in use; restart Loom to use the new version"
        }
    }
    Copy-Item -LiteralPath $src -Destination $dest
}

$loomOut = TargetDir $Repo
$vfsOut = TargetDir $OpenVfs
Install-File (Join-Path $loomOut 'loom.exe') (Join-Path $Prefix 'loom.exe')
Install-File (Join-Path $vfsOut 'openvfs-inject.exe') (Join-Path $Prefix 'openvfs-inject.exe')
Install-File (Join-Path $vfsOut 'openvfs_hook.dll') (Join-Path $Prefix 'openvfs_hook.dll')
if (-not $CliOnly) {
    $deskOut = TargetDir (Join-Path $Repo 'apps\desktop\src-tauri')
    Install-File (Join-Path $deskOut 'loom-desktop.exe') (Join-Path $Prefix 'loom-desktop.exe')
}
# A copy of this script, so Apps & features can uninstall without the checkout.
if (-not (Same-Path $PSScriptRoot $Prefix)) { Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $Prefix 'install.ps1') -Force }

$version = (Get-Content -LiteralPath (Join-Path $Repo 'apps\desktop\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
[ordered]@{
    version = $version; loom = $source.loom; openvfs = $source.openvfs; profile = $Profile_
    source = $Repo; installed = (Get-Date).ToString('s')
} | ConvertTo-Json | Set-Content -LiteralPath $InfoFile -Encoding UTF8

# ---------------------------------------------------------------- integration

if (-not $NoIntegration) {
    Step 'Integrating with Windows'
    if (Edit-UserPath { param($p) if ($p | Where-Object { Same-Path $_ $Prefix }) { $p } else { @($p) + $Prefix } }) {
        Note "added $Prefix to your PATH (new terminals see ``loom``)"
    }
    if (-not (($env:Path -split ';') | Where-Object { $_ -and (Same-Path $_ $Prefix) })) { $env:Path += ";$Prefix" }

    $desktopExe = Join-Path $Prefix 'loom-desktop.exe'
    if (Test-Path $desktopExe) {
        $shell = New-Object -ComObject WScript.Shell
        $lnk = $shell.CreateShortcut($StartMenu)
        $lnk.TargetPath = $desktopExe
        $lnk.WorkingDirectory = $Prefix
        $lnk.IconLocation = "$desktopExe,0"
        $lnk.Description = 'Mod manager that never touches your game folder'
        $lnk.Save()
        Note "Start menu: $Id"
    }

    $size = [int]((Get-ChildItem -LiteralPath $Prefix -File | Measure-Object Length -Sum).Sum / 1KB)
    New-Item -Path $UninstallKey -Force | Out-Null
    $entry = @{
        DisplayName = $Id; DisplayVersion = "$version (dev $($source.loom.Substring(0, 7)))"; Publisher = 'Loom contributors'
        InstallLocation = $Prefix; URLInfoAbout = 'https://github.com/enrell/loom'
        DisplayIcon = if (Test-Path $desktopExe) { $desktopExe } else { Join-Path $Prefix 'loom.exe' }
        UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$Prefix\install.ps1`" -Uninstall -Prefix `"$Prefix`" -Id `"$Id`""
    }
    foreach ($k in $entry.Keys) { Set-ItemProperty -Path $UninstallKey -Name $k -Value $entry[$k] }
    foreach ($k in 'NoModify', 'NoRepair') { New-ItemProperty -Path $UninstallKey -Name $k -Value 1 -PropertyType DWord -Force | Out-Null }
    New-ItemProperty -Path $UninstallKey -Name EstimatedSize -Value $size -PropertyType DWord -Force | Out-Null
    Note 'Apps & features: uninstall entry'

    if ($Nxm) {
        if (Test-Path $desktopExe) { Register-Nxm $desktopExe } else { Note 'skipped -Nxm: the desktop app is not installed' }
    } elseif (-not (Get-NxmCommand)) {
        Note 'tip: -Nxm (or Settings in the app) makes Loom open Nexus "Mod Manager Download" links'
    }
}

Write-Host "Loom $version ($($source.loom.Substring(0, 7))) installed." -ForegroundColor Green
if (-not $CliOnly) { Note "Start it from the Start menu (`"$Id`") or run loom-desktop" }
Note 'CLI: loom --help    Update: scripts\install.ps1 -Update    Remove: scripts\install.ps1 -Uninstall'
if (-not (Get-Command winfsp-x64.dll -ErrorAction SilentlyContinue) -and
    -not (Test-Path "${env:ProgramFiles(x86)}\WinFsp\bin\winfsp-x64.dll")) {
    Note 'WinFsp not found: games launch in hook mode. For mount mode: winget install WinFsp.WinFsp'
}
