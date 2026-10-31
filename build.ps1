$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    if (-not (Test-Path -LiteralPath 'node_modules\@tauri-apps\cli')) {
        npm ci
        if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' }
    cargo build --release --locked -p runbook-cli
    if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
    New-Item -ItemType Directory -Path 'dist' -Force | Out-Null
    Copy-Item -LiteralPath 'target\release\runbook-desktop.exe' -Destination 'dist\Runbook-Desktop.exe' -Force
    Copy-Item -LiteralPath 'target\release\runbook.exe' -Destination 'dist\runbook.exe' -Force
    Copy-Item -LiteralPath 'AI_GUIDE.md' -Destination 'dist\AI_GUIDE.md' -Force
    New-Item -ItemType Directory -Path 'dist\docs' -Force | Out-Null
    Copy-Item -LiteralPath 'docs\CLI.md','docs\ARCHITECTURE.md' -Destination 'dist\docs' -Force
    $taskPairs = @(
        @('target\release\runbook-desktop.exe','dist\Runbook-Desktop.exe'),
        @('target\release\runbook.exe','dist\runbook.exe')
    )
    foreach ($taskPair in $taskPairs) {
        if ((Get-FileHash -LiteralPath $taskPair[0]).Hash -ne (Get-FileHash -LiteralPath $taskPair[1]).Hash) { throw 'Copied executable hash mismatch' }
    }
    Write-Host "Ready: $PSScriptRoot\dist\Runbook-Desktop.exe"
    Write-Host "CLI:   $PSScriptRoot\dist\runbook.exe"
}
finally { Pop-Location }
