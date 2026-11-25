@echo off
rem Builds ika-boost.jar (Java agent). Needs a JDK 17+ on PATH or JAVA_HOME.
setlocal
set ROOT=%~dp0
set ASM=%APPDATA%\IkaLauncher\libraries\org\ow2\asm\asm\9.9.1\asm-9.9.1.jar
set OUT=%ROOT%build
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%\classes"
javac --release 17 -cp "%ASM%" -d "%OUT%\classes" "%ROOT%src\ika\boost\Agent.java" "%ROOT%src\ika\boost\Hook.java" "%ROOT%src\ika\boost\Patch.java" "%ROOT%src\ika\boost\Packages.java" || exit /b 1
> "%OUT%\MANIFEST.MF" echo Premain-Class: ika.boost.Agent
jar --create --file "%ROOT%..\..\src\minecraft\ika-boost.jar" --manifest "%OUT%\MANIFEST.MF" -C "%OUT%\classes" . || exit /b 1


