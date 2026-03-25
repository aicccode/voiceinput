@echo off
chcp 65001 >nul
echo ============================================
echo   VoiceInput Windows Build Script
echo ============================================
echo.

:: Check Rust
where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Rust not found! Install from https://rustup.rs/
    pause
    exit /b 1
)

:: Check clang (needed by whisper-rs-sys)
where clang >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] LLVM/Clang not found!
    echo   Install: winget install LLVM.LLVM
    echo   Or download from: https://github.com/llvm/llvm-project/releases
    pause
    exit /b 1
)

cd /d "%~dp0src-tauri"

:: Check if model exists for release build
if exist "..\ggml-small.bin" (
    echo [INFO] Found ggml-small.bin, building RELEASE with embedded model...
    echo [INFO] This will take a while and produce a ~500MB single-file exe.
    echo.
    cargo build --release
    if %errorlevel% neq 0 (
        echo [ERROR] Build failed!
        pause
        exit /b 1
    )
    echo.
    echo ============================================
    echo   BUILD SUCCESS (Release)
    echo   Output: src-tauri\target\release\voiceinput.exe
    echo   Single file, model embedded, ready to distribute.
    echo ============================================
) else (
    echo [INFO] ggml-small.bin not found in project root.
    echo [INFO] Building DEV mode (model NOT embedded)...
    echo.
    set VOICEINPUT_DEV=1
    cargo build
    if %errorlevel% neq 0 (
        echo [ERROR] Build failed!
        pause
        exit /b 1
    )
    echo.
    echo ============================================
    echo   BUILD SUCCESS (Dev Mode)
    echo   Output: src-tauri\target\debug\voiceinput.exe
    echo.
    echo   You need a model file to run! Place one of these
    echo   next to voiceinput.exe:
    echo     - ggml-base.bin  (~141MB, faster, less accurate)
    echo     - ggml-small.bin (~466MB, recommended)
    echo ============================================
)

echo.
pause
