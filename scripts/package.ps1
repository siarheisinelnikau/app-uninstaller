param([switch]$Offline)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repo
try {
    $cargoFlags = @('--release', '--locked', '--target', 'x86_64-pc-windows-msvc', '-j', '2')
    if ($Offline) { $cargoFlags += '--offline' }
    & cargo build @cargoFlags
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $versionLine = Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"$' | Select-Object -First 1
    $version = $versionLine.Matches[0].Groups[1].Value
    $asset = "AppUninstaller-v$version.exe"
    New-Item -ItemType Directory -Path 'dist' -Force | Out-Null
    Copy-Item -LiteralPath 'target/x86_64-pc-windows-msvc/release/AppUninstaller.exe' -Destination (Join-Path 'dist' $asset)
    Copy-Item -LiteralPath 'LICENSE', 'THIRD_PARTY_NOTICES.txt' -Destination 'dist'
    $digest = (Get-FileHash -LiteralPath (Join-Path 'dist' $asset) -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath 'dist/SHA256SUMS.txt' -Value "$digest  $asset" -Encoding ascii
    Write-Output "Prepared dist/$asset and SHA256SUMS.txt. Nothing was launched or published."
} finally { Pop-Location }
