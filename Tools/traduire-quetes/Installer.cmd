@echo off
rem Installe le traducteur hors ligne (Python, CTranslate2, modèle NLLB). Exemple : Installer.cmd -Modele 600M
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Installer.ps1" %*
pause
