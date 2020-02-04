@echo off
echo *** Vector command prompt environment ***
echo.
echo Start Vector by running
echo     vector --config config\vector.toml
echo or use
echo     vector --help
echo to get help.
cd %~dp0
cmd /k set PATH=%~dp0bin;%PATH%
