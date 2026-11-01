@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0
IF EXIST "%dp0%\kamish.exe" (
  FOR %%A IN ("%dp0%\kamish.exe") DO IF %%~zA GTR 1024 (
    "%dp0%\kamish.exe" %*
    EXIT /b %ERRORLEVEL%
  )
)
IF EXIST "%dp0%\..\..\kamish\bin\kamish.js" (
  node "%dp0%\..\..\kamish\bin\kamish.js" %*
  EXIT /b %ERRORLEVEL%
)
node -e "console.error('Kamish CLI runtime not found.')"
EXIT /b 1
