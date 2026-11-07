param([switch]$SignUpdate, [string]$ReleaseNotes)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    node scripts/check-version.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Release versions are inconsistent' }
    if (-not (Test-Path -LiteralPath 'node_modules\@tauri-apps\cli')) {
        npm ci
        if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' }
    cargo build --release --locked -p runbook-cli
    if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
    cargo build --release --locked -p runbook-update --bin runbook-updater
    if ($LASTEXITCODE -ne 0) { throw 'Updater build failed' }
    New-Item -ItemType Directory -Path 'dist' -Force | Out-Null
    Copy-Item -LiteralPath 'target\release\runbook-desktop.exe' -Destination 'dist\Runbook-Desktop.exe' -Force
    Copy-Item -LiteralPath 'target\release\runbook.exe' -Destination 'dist\runbook.exe' -Force
    Copy-Item -LiteralPath 'target\release\runbook-updater.exe' -Destination 'dist\runbook-updater.exe' -Force
    Copy-Item -LiteralPath 'AI_GUIDE.md' -Destination 'dist\AI_GUIDE.md' -Force
    Copy-Item -LiteralPath 'docs\QUICKSTART.md' -Destination 'dist\README.md' -Force
    Copy-Item -LiteralPath 'LICENSE' -Destination 'dist\LICENSE' -Force
    New-Item -ItemType Directory -Path 'dist\docs' -Force | Out-Null
    Copy-Item -LiteralPath 'docs\CLI.md','docs\ARCHITECTURE.md','docs\UPDATES.md' -Destination 'dist\docs' -Force
    New-Item -ItemType Directory -Path 'dist\skills\runbook-cli' -Force | Out-Null
    Copy-Item -LiteralPath 'skills\runbook-cli\SKILL.md' -Destination 'dist\skills\runbook-cli\SKILL.md' -Force
    New-Item -ItemType Directory -Path 'dist\scripts' -Force | Out-Null
    Copy-Item -LiteralPath 'scripts\register-cli.ps1','scripts\register-cli.cmd' -Destination 'dist\scripts' -Force
    $taskPairs = @(
        @('target\release\runbook-desktop.exe','dist\Runbook-Desktop.exe'),
        @('target\release\runbook.exe','dist\runbook.exe'),
        @('target\release\runbook-updater.exe','dist\runbook-updater.exe')
    )
    foreach ($taskPair in $taskPairs) {
        if ((Get-FileHash -LiteralPath $taskPair[0]).Hash -ne (Get-FileHash -LiteralPath $taskPair[1]).Hash) { throw 'Copied executable hash mismatch' }
    }
    Write-Host "Ready: $PSScriptRoot\dist\Runbook-Desktop.exe"
    Write-Host "CLI:   $PSScriptRoot\dist\runbook.exe"
    & (Join-Path $PSScriptRoot 'scripts\package-portable.ps1')
    if ($SignUpdate) {
        $taskSignArgs = @('scripts/sign-update.mjs')
        if ($ReleaseNotes) { $taskSignArgs += @('--notes', $ReleaseNotes) }
        node @taskSignArgs
        if ($LASTEXITCODE -ne 0) { throw 'Update signing failed' }
    }
}
finally { Pop-Location }
