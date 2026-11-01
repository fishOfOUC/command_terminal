[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskDist = Join-Path $taskRoot 'dist'
$taskQa = Join-Path $taskRoot '.qa'
$taskStage = Join-Path $taskQa ('portable-package-' + [guid]::NewGuid().ToString('N'))
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskRelease = Join-Path $taskRoot 'release'
$taskArchive = Join-Path $taskRelease ('Runbook-' + $taskVersion + '-windows-x64.zip')
New-Item -ItemType Directory -Path $taskStage,$taskRelease -Force | Out-Null
# Explicit release contents: local databases and WebView profiles must never be distributed.
foreach ($taskEntry in @('Runbook-Desktop.exe','runbook.exe','README.md','LICENSE','AI_GUIDE.md','docs','skills','scripts')) {
    Copy-Item -LiteralPath (Join-Path $taskDist $taskEntry) -Destination $taskStage -Recurse -Force
}
Compress-Archive -Path (Join-Path $taskStage '*') -DestinationPath $taskArchive -Force
$taskZip = [IO.Compression.ZipFile]::OpenRead($taskArchive)
try {
    if (@($taskZip.Entries | Where-Object { $_.FullName -match '(^|[/\\])data([/\\]|$)|\.db(?:-|$)' }).Count) {
        throw 'Release archive contains local application data'
    }
}
finally { $taskZip.Dispose() }
$taskResolvedStage = (Resolve-Path -LiteralPath $taskStage).Path
if (-not $taskResolvedStage.StartsWith($taskQa + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw ('Refusing to remove unexpected staging directory: ' + $taskResolvedStage)
}
Remove-Item -LiteralPath $taskResolvedStage -Recurse -Force
Write-Output ('Portable release: ' + $taskArchive)
