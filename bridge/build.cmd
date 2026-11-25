@echo off
setlocal
if "%~2"=="" (
    echo Usage: build.cmd Detours-source-directory output-directory
    exit /b 2
)
set "IKA_BRIDGE_DETOURS=%~f1"
set "IKA_BRIDGE_BUILD=%~f2"
if not exist "%IKA_BRIDGE_BUILD%" mkdir "%IKA_BRIDGE_BUILD%"
pushd "%IKA_BRIDGE_DETOURS%\src"
if errorlevel 1 exit /b 1
nmake /nologo
set "IKA_BRIDGE_STATUS=%ERRORLEVEL%"
popd
if not "%IKA_BRIDGE_STATUS%"=="0" exit /b %IKA_BRIDGE_STATUS%
cl /nologo /std:c++17 /O2 /EHsc /MD /W4 /I "%IKA_BRIDGE_DETOURS%\src" /Fo:"%IKA_BRIDGE_BUILD%\producer.obj" /LD "%~dp0producer.cpp" /link /DEF:"%~dp0producer.def" /OUT:"%IKA_BRIDGE_BUILD%\producer.dll" /IMPLIB:"%IKA_BRIDGE_BUILD%\producer.lib" "%IKA_BRIDGE_DETOURS%\lib.X64\detours.lib" user32.lib d3d11.lib dxgi.lib cabinet.lib
if errorlevel 1 exit /b 1
cl /nologo /std:c++17 /O2 /EHsc /MD /W4 /I "%IKA_BRIDGE_DETOURS%\src" /Fo:"%IKA_BRIDGE_BUILD%\host.obj" /Fe:"%IKA_BRIDGE_BUILD%\host.exe" "%~dp0host.cpp" "%IKA_BRIDGE_DETOURS%\lib.X64\detours.lib" user32.lib ole32.lib
if errorlevel 1 exit /b 1
cl /nologo /std:c++17 /O2 /EHsc /MD /W4 /I "%~dp0mpv\include" /Fo:"%IKA_BRIDGE_BUILD%\video.obj" /Fe:"%IKA_BRIDGE_BUILD%\video.exe" "%~dp0video.cpp" d3d11.lib d3dcompiler.lib dxgi.lib mfplat.lib mfuuid.lib ole32.lib oleaut32.lib windowscodecs.lib cabinet.lib user32.lib winmm.lib
if errorlevel 1 exit /b 1
copy /Y "%IKA_BRIDGE_BUILD%\video.exe" "%IKA_BRIDGE_BUILD%\video-mpv.exe" >nul
if errorlevel 1 exit /b 1
copy /Y "%IKA_BRIDGE_BUILD%\video.exe" "%IKA_BRIDGE_BUILD%\video-profile-mpv.exe" >nul
if errorlevel 1 exit /b 1
