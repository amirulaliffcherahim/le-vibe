# lazybug Windows Installer
# Usage: irm https://raw.githubusercontent.com/amirulaliffcherahim/lazybug/main/scripts/install.ps1 | iex
$ErrorActionPreference = "Stop"

$Repo = "amirulaliffcherahim/lazybug"
$InstallDir = Join-Path $env:USERPROFILE ".lazybug\bin"
$ExePath = Join-Path $InstallDir "lazybug.exe"

Write-Host "Fetching latest lazybug release from GitHub..." -ForegroundColor Cyan
$Release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$Asset = $Release.assets | Where-Object { $_.name -eq "lazybug.exe" } | Select-Object -First 1
if (-not $Asset) {
    throw "No lazybug.exe asset found on release $($Release.tag_name)."
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Write-Host "Downloading $($Release.tag_name) -> $ExePath" -ForegroundColor Cyan
Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ExePath -UseBasicParsing

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "Added $InstallDir to User PATH." -ForegroundColor Green
}

Write-Host "Installed lazybug $($Release.tag_name)!" -ForegroundColor Green
& $ExePath --version
