@echo off
:: cuvm.cmd — cmd.exe shim (degraded: no cd-hook). Put this dir on PATH.
:: Activation verbs write a temp .bat, CALL it, then DEL it (print-then-eval, §8).
set "_CUVM_VERB=%~1"
if /I "%_CUVM_VERB%"=="use"     goto :emit
if /I "%_CUVM_VERB%"=="env"     goto :emit
if /I "%_CUVM_VERB%"=="shell"   goto :emit
if /I "%_CUVM_VERB%"=="default" goto :emit
cuvm.exe %*
goto :eof
:emit
set "_CUVM_TMP=%TEMP%\cuvm-%RANDOM%.bat"
cuvm.exe env %* --shell cmd --out "%_CUVM_TMP%" && call "%_CUVM_TMP%" && del "%_CUVM_TMP%"
set "_CUVM_TMP="
set "_CUVM_VERB="
