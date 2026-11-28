@echo off
rem Opens the Vilocity launcher (scripts\launcher.ps1) for a first start, before the engine is built.
rem Once it is built, use the "Vilocity" shortcut next to this file: no console window at all.
if exist "%~dp0target\release\vilocity-launcher.exe" (
    start "" "%~dp0target\release\vilocity-launcher.exe"
) else (
    start "" powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0scripts\launcher.ps1"
)
