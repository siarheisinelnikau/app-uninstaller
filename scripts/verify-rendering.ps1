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
    Start-Sleep -Seconds 1
    $process.Refresh()
    $graphicsModules = @($process.Modules | Where-Object { $_.ModuleName -match '^(opengl|d3d|dxgi|vulkan)' } | Select-Object -ExpandProperty ModuleName)
    if (-not $process.WaitForExit(39000)) {
        $process.Kill()
        throw 'Disposable rendering verification exceeded its 40-second timeout'
    }
    if ($process.ExitCode -ne 0) { throw "Rendering verification failed: $($process.ExitCode)" }
    $result = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    if ($result.pid -ne $process.Id -or -not $result.fixture) { throw 'Unexpected diagnostic process or workload' }
    if ($result.input_events -ne 0) { throw 'External input contaminated the quiet verification interval' }
    foreach ($phase in @(0, 1, 2)) {
        $quietRun = 0
        $longestQuietRun = 0
        foreach ($sample in @($result.samples | Where-Object { $_.phase -eq $phase })) {
            $expectedVisible = $phase -ne 1
            if ($sample.ui_passes -eq 0 -and $sample.rasterizations -eq 0 -and $sample.presentations -eq 0 -and $sample.visible -eq $expectedVisible) {
                $quietRun += 1
                $longestQuietRun = [Math]::Max($longestQuietRun, $quietRun)
            } else { $quietRun = 0 }
        }
        if ($longestQuietRun -lt 5) { throw "No five-second settled interval in phase $phase" }
    }
    $result | Add-Member -NotePropertyName loaded_graphics_api_modules -NotePropertyValue $graphicsModules
    $result | Add-Member -NotePropertyName binary_sha256 -NotePropertyValue (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding utf8
    Write-Output "Verified PID $($result.pid): zero settled layout/raster/present work in visible, hidden and restored phases. Process exited."
} finally { Pop-Location }
