@echo off
rem Traduit et applique les textes de quêtes. Exemple : Traduire.cmd -Racine C:\Jeux\HomeFrontier
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Traduire.ps1" %*
pause
