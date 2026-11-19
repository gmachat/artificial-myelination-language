param(
  [string]$InstallDir = $(if ($env:AML_INSTALL_DIR) { $env:AML_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "AML\bin" }),
  [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$destination = Join-Path $InstallDir "aml.exe"

if ($Uninstall) {
  if (Test-Path -LiteralPath $destination) {
    Remove-Item -LiteralPath $destination
    Write-Host "Removed $destination"
  } else {
    Write-Host "AML is not installed at $destination"
  }
  exit 0
}

$source = Join-Path $PSScriptRoot "aml.exe"
if (-not (Test-Path -LiteralPath $source)) {
  throw "AML executable not found beside installer: $source"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$temporary = "$destination.install.$PID"
try {
  Copy-Item -LiteralPath $source -Destination $temporary -Force
  Move-Item -LiteralPath $temporary -Destination $destination -Force
} finally {
  if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
}

Write-Host "Installed $destination"
& $destination --version
if (($env:PATH -split [IO.Path]::PathSeparator) -notcontains $InstallDir) {
  Write-Host "Add $InstallDir to PATH to run aml from any directory."
}

