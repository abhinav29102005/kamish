#!/usr/bin/env pwsh
$basedir = Split-Path $MyInvocation.MyCommand.Path -Parent
$exe = Join-Path $basedir "kamish.exe"
if ((Test-Path $exe) -and ((Get-Item $exe).Length -gt 1024)) {
  & $exe $args
} else {
  $js = Join-Path $basedir "..\..\kamish\bin\kamish.js"
  & node $js $args
}
exit $LASTEXITCODE
