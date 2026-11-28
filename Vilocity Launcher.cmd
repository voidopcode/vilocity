@echo off
rem Opens the Vilocity launcher (scripts\launcher.ps1). Normally use the "Vilocity" shortcut or
rem target\release\vilocity-launcher.exe; this is the fallback when an antivirus removed that exe.
if exist "%~dp0target\release\vilocity-launcher.exe" (
    start "" "%~dp0target\release\vilocity-launcher.exe"
) else (
    start "" powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0scripts\launcher.ps1"
)
