@echo off
REM ============================================
REM  Trading App - Full Build Script
REM  Builds frontend + backend from source
REM ============================================

cd /d "%~dp0"

echo.
echo === Step 1: Building Frontend ===
echo.

cd frontend
call npm install
if %errorlevel% neq 0 (
    echo ERROR: npm install failed
    pause
    exit /b 1
)
call npm run build
if %errorlevel% neq 0 (
    echo ERROR: frontend build failed
    pause
    exit /b 1
)
cd ..

echo.
echo === Step 2: Building Backend ===
echo.

cargo build
if %errorlevel% neq 0 (
    echo ERROR: backend build failed
    pause
    exit /b 1
)

echo.
echo === Build Complete! ===
echo.
echo To start the server, run: start.bat
echo Then open: http://localhost:8080
echo.
pause
