@echo off
REM ---------------------------------------------------------------------------
REM truss-fem convenience launcher (Windows).
REM
REM   run.bat                     Build (release) then start the interactive
REM                               shell. Inside it, type: help, samples,
REM                               load <file>, solve, view, save <file>, exit.
REM   run.bat demo                Build then run a quick demo:
REM                               solve + open the web viewer for a sample.
REM   run.bat <args...>           Build then forward all args to truss-fem.
REM                               e.g.  run.bat solve input\bridge_pratt_2d.txt
REM                                     run.bat view input\building_tower_3d.txt --open
REM ---------------------------------------------------------------------------
setlocal
cd /d "%~dp0"

set BIN=target\release\truss-fem.exe

echo [truss-fem] Building (release)...
cargo build --release
if errorlevel 1 (
  echo [truss-fem] Build failed.
  exit /b 1
)

if "%~1"=="" (
  "%BIN%" shell
) else if /i "%~1"=="demo" (
  echo [truss-fem] Demo: solve + web viewer for input\bridge_pratt_2d.txt
  "%BIN%" solve input\bridge_pratt_2d.txt
  "%BIN%" view  input\bridge_pratt_2d.txt --open
) else (
  "%BIN%" %*
)

endlocal
