@echo off
setlocal EnableExtensions
cd /d "%~dp0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\run-all-tests.ps1"
set "QA_EXIT=%ERRORLEVEL%"
echo.
if not "%QA_EXIT%"=="0" (
  echo FAILED with exit code %QA_EXIT%.
) else (
  echo PASS: all Windows tests and self-hardening completed.
)
if not "%QA_NO_PAUSE%"=="1" pause
exit /b %QA_EXIT%
