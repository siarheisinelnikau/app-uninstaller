param([string]$OutputDirectory = 'docs/screenshots')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repo
try {
    $versionLine = Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"$' | Select-Object -First 1
    $version = $versionLine.Matches[0].Groups[1].Value
    $exe = Join-Path $repo "dist/AppUninstaller-v$version.exe"
    if (-not (Test-Path -LiteralPath $exe)) { throw 'Package the application before capture' }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $output = (Resolve-Path -LiteralPath $OutputDirectory).Path
    foreach ($screen in @('applications', 'settings', 'leftovers', 'applications-unavailable', 'applications-removing', 'applications-removed', 'applications-collapsing')) {
        $path = Join-Path $output "$screen.png"
        $process = Start-Process -FilePath $exe -ArgumentList @('--screenshot', $path, '--screen', $screen) -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit(20000)) {
            $process.Kill()
            throw "Disposable $screen capture exceeded its 20-second timeout"
        }
        if ($process.ExitCode -ne 0) { throw "$screen capture failed: $($process.ExitCode)" }
        if (-not (Test-Path -LiteralPath $path)) { throw "Missing $screen capture" }
        Write-Output "${screen}: saved actual CPU-rendered client area and exited"
    }
} finally { Pop-Location }
