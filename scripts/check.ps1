param([switch]$Offline)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repo
try {
    $cargoFlags = @('--locked', '-j', '2')
    if ($Offline) { $cargoFlags += '--offline' }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting failed' }
    & cargo clippy @cargoFlags --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    # All tests use inert files inside target/test-fixtures. No inventory/process effects.
    & cargo test @cargoFlags --all-targets
    if ($LASTEXITCODE -ne 0) { throw 'Fixture tests failed' }
} finally { Pop-Location }
