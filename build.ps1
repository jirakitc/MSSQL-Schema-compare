param([switch]$Debug, [string]$OutputDirectory = 'dist\SchemaCompare')
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$env:PATH = "$PSScriptRoot\.tools\mingw64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
if (-not (Test-Path -LiteralPath "$PSScriptRoot\.tools\mingw64\bin\gcc.exe")) {
    throw 'Portable MinGW is missing. See README.md for toolchain setup or use the MSVC build instructions.'
}
$buildArguments = @('build','--locked','--bins')
$profile = 'debug'
if (-not $Debug) { $buildArguments += '--release'; $profile = 'release' }
& cargo @buildArguments
if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed' }
$destination = Join-Path $PSScriptRoot $OutputDirectory
New-Item -ItemType Directory -Force "$destination\samples" | Out-Null
Copy-Item "target\x86_64-pc-windows-gnu\$profile\schema-compare.exe","target\x86_64-pc-windows-gnu\$profile\schema-compare-cli.exe" $destination
Copy-Item samples\*.json "$destination\samples"
Copy-Item README.md,THIRD_PARTY_NOTICES.md $destination
New-Item -ItemType Directory -Force "$destination\assets" | Out-Null
Copy-Item assets\schema-compare.ico,assets\schema-compare.png,assets\schema-compare.svg "$destination\assets"
Write-Output "Built: $destination\schema-compare.exe"
