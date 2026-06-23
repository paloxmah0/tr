@echo off
REM ============================================
REM  Trading App - Quick Start (no recompiling!)
REM  Runs the pre-built binary directly.
REM  Run build.bat first if you haven't built yet.
REM ============================================

cd /d "%~dp0"

if not exist "target\debug\trading-backend.exe" (
    echo Binary not found. Run build.bat first.
    pause
    exit /b 1
)

if not exist "frontend\dist\index.html" (
    echo Frontend not built. Run build.bat first.
    pause
    exit /b 1
)

echo Starting Trading App...
echo.
echo Open your browser to: http://localhost:8080
echo.
echo Press Ctrl+C to stop the server.
echo.

target\debug\trading-backend.exe

pause
