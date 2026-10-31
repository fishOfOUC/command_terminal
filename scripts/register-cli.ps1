[CmdletBinding()]
param(
    [string]$CliPath,
    [string]$BinDirectory = (Join-Path ([Environment]::GetFolderPath('UserProfile')) '.local\bin')
)
$ErrorActionPreference = 'Stop'
if (-not $CliPath) {
    $taskRepositoryCli = Join-Path $PSScriptRoot '..\dist\runbook.exe'
    $CliPath = if (Test-Path -LiteralPath $taskRepositoryCli -PathType Leaf) { $taskRepositoryCli } else { Join-Path $PSScriptRoot '..\runbook.exe' }
}
$taskCli = (Resolve-Path -LiteralPath $CliPath).Path
if (-not (Test-Path -LiteralPath $taskCli -PathType Leaf)) { throw "CLI not found: $taskCli" }
if ($taskCli -match '[%\r\n"]') { throw 'CLI path contains characters unsupported by a Windows command launcher' }
$taskBin = [IO.Path]::GetFullPath($BinDirectory)
New-Item -ItemType Directory -Path $taskBin -Force | Out-Null
$taskLauncher = Join-Path $taskBin 'runbook.cmd'
$taskMarker = 'rem Runbook CLI launcher (managed by register-cli.ps1)'
if ((Test-Path -LiteralPath $taskLauncher) -and -not ([IO.File]::ReadAllText($taskLauncher).Contains($taskMarker))) {
    throw "Another launcher already exists: $taskLauncher"
}
$taskContent = "@echo off`r`n$taskMarker`r`n@`"$taskCli`" %*`r`n"
[IO.File]::WriteAllText($taskLauncher, $taskContent, [Text.Encoding]::Default)
$taskUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$taskAlreadyRegistered = @($taskUserPath -split ';' | ForEach-Object { $_.Trim().TrimEnd('\') }) -contains $taskBin.TrimEnd('\')
if (-not $taskAlreadyRegistered) {
    $taskNewPath = if ([string]::IsNullOrWhiteSpace($taskUserPath)) { $taskBin } else { $taskUserPath.TrimEnd(';') + ';' + $taskBin }
    [Environment]::SetEnvironmentVariable('Path', $taskNewPath, 'User')
}
if (-not (@($env:Path -split ';' | ForEach-Object { $_.Trim().TrimEnd('\') }) -contains $taskBin.TrimEnd('\'))) {
    $env:Path = $taskBin + ';' + $env:Path
}
Write-Output "Registered: $taskLauncher"
Write-Output "Target:     $taskCli"
Write-Output 'Use runbook in a new terminal; existing terminals can use the launcher immediately if its directory is already on PATH.'
