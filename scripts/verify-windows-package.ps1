param([Parameter(Mandatory=$true)][string]$Target)
$ErrorActionPreference = 'Stop'
$config = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$executable = "src-tauri/target/$Target/release/ClashNova.exe"
$version = (Get-Item $executable).VersionInfo
if ($version.ProductName -ne 'ClashNova') { throw "Unexpected product name: $($version.ProductName)" }
if (-not $version.FileVersion.StartsWith($config.version)) { throw "Unexpected file version: $($version.FileVersion)" }
if ($version.CompanyName -ne 'clashnova-dev') { throw "Unexpected publisher: $($version.CompanyName)" }
Add-Type -AssemblyName System.Drawing
$expected = [System.Drawing.Icon]::new((Resolve-Path src-tauri/icons/icon.ico), 32, 32).ToBitmap()
New-Item -ItemType Directory -Force release-artifacts | Out-Null
$installer = Get-ChildItem "src-tauri/target/$Target/release/bundle/nsis/*-setup.exe"
if ($installer.Count -ne 1) { throw 'Expected one Windows installer' }
foreach ($entry in @(@{path=$executable; name='application'}, @{path=$installer.FullName; name='installer'})) {
    $icon = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path $entry.path)).ToBitmap()
    if ($icon.Width -ne 32 -or $icon.Height -ne 32) { throw 'Unexpected icon dimensions' }
    for ($x=0; $x -lt 32; $x++) {
        for ($y=0; $y -lt 32; $y++) {
            if ($icon.GetPixel($x,$y).ToArgb() -ne $expected.GetPixel($x,$y).ToArgb()) {
                throw "Packaged $($entry.name) icon does not match ClashNova icon"
            }
        }
    }
    $icon.Save((Join-Path (Resolve-Path release-artifacts) "icon-$($entry.name).png"))
    $icon.Dispose()
}
$expected.Dispose()
Write-Output "Verified ClashNova $($version.FileVersion), publisher clashnova-dev, application and installer icons"
