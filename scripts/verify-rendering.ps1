param([string]$Report = '.local/gui-qa/rendering-final.json')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repo
try {
    $versionLine = Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"$' | Select-Object -First 1
    $version = $versionLine.Matches[0].Groups[1].Value
    $exe = Join-Path $repo "dist/AppUninstaller-v$version.exe"
    $reportPath = [IO.Path]::GetFullPath((Join-Path $repo $Report))
    New-Item -ItemType Directory -Path (Split-Path -Parent $reportPath) -Force | Out-Null
    # An isolated fixture window deliberately shows, hides and restores itself, then exits.
    $process = Start-Process -FilePath $exe -ArgumentList @('--verify-rendering', $reportPath) -PassThru
    if (-not $process.WaitForExit(24000)) {
        $process.Kill()
        throw 'Disposable rendering verification exceeded its 24-second timeout'
    }
    if ($process.ExitCode -ne 0) { throw "Rendering verification failed: $($process.ExitCode)" }
    $result = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    if ($result.pid -ne $process.Id -or -not $result.fixture) { throw 'Unexpected diagnostic process or workload' }
    if ($result.input_events -ne 0) { throw 'External input contaminated the quiet verification interval' }
    foreach ($phase in @(0, 1, 2)) {
        $settled = @($result.samples | Where-Object { $_.phase -eq $phase } | Select-Object -Skip 1)
        if ($settled.Count -lt 4) { throw "Missing settled samples for phase $phase" }
        foreach ($sample in $settled) {
            if ($sample.ui_passes -ne 0 -or $sample.rasterizations -ne 0 -or $sample.presentations -ne 0) {
                throw "Unnecessary rendering in settled phase $phase"
            }
        }
    }
    Write-Output "Verified PID $($result.pid): zero settled layout/raster/present work in visible, hidden and restored phases. Process exited."
} finally { Pop-Location }
