@echo off
rem GammaEngine server with its console, for testing. Written by tools/test-server/setup.ps1,
rem which keeps this file once it exists: edit it freely.
rem   JAVA       java.exe to use. Default: java on the PATH (Java 8, like production).
rem              Any Java 9 or later gets the arguments of java9args.txt.
rem   MEMORY     heap, used for -Xms and -Xmx. Default: 4G.
rem   JVM_ARGS   extra JVM arguments, for example -XX:+UseZGC -XX:+ZGenerational on Java 21.
rem Example: set JAVA=%USERPROFILE%\.jdks\ms-21.0.12.1\bin\java.exe
setlocal
cd /d "%~dp0"
if not defined JAVA set "JAVA=java"
if not defined MEMORY set "MEMORY=4G"
set "MODULES="
"%JAVA%" -version 2>&1 | findstr /r /c:"version .1\." >nul || set "MODULES=@java9args.txt"
rem Java 9 and later write UTF-8 (java9args.txt): switch the console to match.
if defined MODULES chcp 65001 >nul
title GammaEngine - %CD%
echo [GammaEngine] "%JAVA%" -Xms%MEMORY% -Xmx%MEMORY% %MODULES% %JVM_ARGS% -jar server.jar nogui
"%JAVA%" -Xms%MEMORY% -Xmx%MEMORY% %MODULES% %JVM_ARGS% -jar server.jar nogui
echo.
echo [GammaEngine] Server stopped, exit code %ERRORLEVEL%.
pause
