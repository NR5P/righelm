# Build RigHelm and install it for the current user, with Start menu and
# desktop shortcuts. No administrator rights needed.
#
#   powershell -ExecutionPolicy Bypass -File packaging\install-windows.ps1 [-Dest <folder>] [-NoBuild]
#
# -NoBuild installs what's already in target\release. From a release download
# (no source next to it) the .exe files beside it are installed.
param(
    [string]$Dest = "$env:LOCALAPPDATA\Programs\RigHelm",
    [switch]$NoBuild
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$fromSource = Test-Path "$root\Cargo.toml"
if ($fromSource -and -not $NoBuild) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
$bin = if (Test-Path "$root\target\release\righelm.exe") { "$root\target\release" } else { $root }
if (-not (Test-Path "$bin\righelm.exe")) { throw "righelm.exe not found; build it first" }

# RigHelm may be running from the folder we're about to overwrite.
$running = Get-Process righelm, righelmd -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($Dest, [StringComparison]::OrdinalIgnoreCase) }
if ($running) {
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force $Dest | Out-Null
# Windows can hold a just-closed program's file a moment longer.
for ($try = 1; ; $try++) {
    try { Copy-Item "$bin\righelm.exe", "$bin\righelmd.exe" $Dest -Force; break }
    catch { if ($try -ge 20) { throw }; Start-Sleep -Milliseconds 500 }
}
Copy-Item "$root\README.md", "$root\LICENSE" $Dest -Force

# Icon for the shortcuts: an .ico holding the PNGs as they are (Vista and
# later read PNG entries).
$sizes = 32, 48, 64, 128, 256
$pngs = $sizes | ForEach-Object { , [IO.File]::ReadAllBytes("$root\assets\righelm-$_.png") }
$ico = New-Object IO.MemoryStream
$w = New-Object IO.BinaryWriter $ico
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $n = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$n); $w.Write([byte]$n); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32)
    $w.Write([UInt32]$pngs[$i].Length); $w.Write([UInt32]$offset)
    $offset += $pngs[$i].Length
}
foreach ($p in $pngs) { $w.Write($p) }
$w.Flush()
[IO.File]::WriteAllBytes("$Dest\righelm.ico", $ico.ToArray())

$shell = New-Object -ComObject WScript.Shell
$links = @(
    "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\RigHelm.lnk",
    "$([Environment]::GetFolderPath('Desktop'))\RigHelm.lnk"
)
foreach ($l in $links) {
    $s = $shell.CreateShortcut($l)
    $s.TargetPath = "$Dest\righelm.exe"
    $s.WorkingDirectory = $Dest
    $s.IconLocation = "$Dest\righelm.ico,0"
    $s.Description = "RigHelm rig control"
    $s.Save()
}

Write-Host "Installed RigHelm to $Dest (Start menu and desktop shortcuts added)."
