@echo off
rem Opens the Vilocity launcher (scripts\launcher.ps1): pick the game folder, set the options, press PLAY.
start "" powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0scripts\launcher.ps1"
