@echo off
setlocal
rem Register the CLI for the current Windows user; no administrator privileges required.
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0register-cli.ps1" %*
exit /b %errorlevel%
