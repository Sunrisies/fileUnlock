@echo off
setlocal enabledelayedexpansion

set TARGET_DIR=%USERPROFILE%\.fileunlock
set EXE_SRC=target\release\FileUnlock.exe

if not exist "%EXE_SRC%" (
    echo [错误] 请先在项目目录下运行 cargo build --release
    exit /b 1
)

if not exist "%TARGET_DIR%" mkdir "%TARGET_DIR%"
copy /Y "%EXE_SRC%" "%TARGET_DIR%\FileUnlock.exe" >nul
echo [OK] 已复制 %TARGET_DIR%\FileUnlock.exe

:: 创建快捷别名
copy /Y "%TARGET_DIR%\FileUnlock.exe" "%TARGET_DIR%\inuse.exe" >nul
copy /Y "%TARGET_DIR%\FileUnlock.exe" "%TARGET_DIR%\who.exe" >nul
echo [OK] 别名: inuse.exe, who.exe

:: 检查 PATH 是否已包含
set OLD_PATH=%TARGET_DIR:\=\\%
echo %PATH% | findstr /I /C:"%TARGET_DIR%" >nul
if !errorlevel! equ 0 (
    echo [OK] 已在 PATH 中
) else (
    :: 添加到用户 PATH
    for /f "usebackq tokens=2,*" %%a in (`reg query HKCU\Environment /v Path 2^>nul`) do (
        set USER_PATH=%%b
    )
    setx Path "%TARGET_DIR%;!USER_PATH!" >nul
    echo [OK] 已添加到 PATH，请重新打开终端后生效
)

echo.
echo 安装完成！你现在可以在任何目录下使用：
echo.
echo   inuse --help             查看帮助
echo   inuse check D:\文件.txt  检查文件
echo   inuse ps notepad         搜索进程
echo   inuse kill 61928         结束进程
echo   inuse where node         查找程序位置
echo.
echo 三个名字等价: inuse ^= who ^= FileUnlock
