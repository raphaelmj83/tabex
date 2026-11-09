@echo off
setlocal EnableExtensions
rem ============================================================
rem  Tabex - instalador / atualizador para Windows
rem  Instala (ou atualiza) o Tabex em C:\tabex a partir da release
rem  mais recente do GitHub. Nao precisa de administrador.
rem
rem  Uso:  instalar.bat            (baixa a ultima versao)
rem        instalar.bat caminho\tabex-windows-x64-vX.Y.Z.zip
rem  Se houver um tabex-windows-x64-*.zip ao lado deste .bat,
rem  ele e usado sem acessar a internet.
rem
rem  Repositorio privado: salve um token do GitHub (somente leitura
rem  de releases) em C:\tabex\github_token.txt ou na variavel de
rem  ambiente TABEX_GITHUB_TOKEN.
rem ============================================================
set "DEST=C:\tabex"
set "REPO=raphaelmj83/tabex"
set "ZIP=%~1"

echo.
echo  Tabex - instalacao em %DEST%
echo.
if not exist "%DEST%" mkdir "%DEST%" || goto :fail

rem Fecha o Tabex se estiver aberto (o executavel nao pode ser substituido em uso).
tasklist /FI "IMAGENAME eq tabex.exe" 2>nul | find /I "tabex.exe" >nul && (
    echo  Fechando o Tabex em execucao...
    taskkill /IM tabex.exe /F >nul 2>&1
    timeout /t 1 /nobreak >nul
)

if defined ZIP if exist "%ZIP%" goto :extract
set "ZIP="
for %%f in ("%~dp0tabex-windows-x64-*.zip") do set "ZIP=%%~ff"
if defined ZIP (
    echo  Usando pacote local: %ZIP%
    goto :extract
)

echo  Baixando a versao mais recente de github.com/%REPO% ...
set "ZIP=%TEMP%\tabex-download.zip"
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference='Stop';" ^
  "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12;" ^
  "$h=@{'User-Agent'='tabex-installer';'Accept'='application/vnd.github+json'};" ^
  "$tok=$env:TABEX_GITHUB_TOKEN;" ^
  "if(-not $tok -and (Test-Path '%DEST%\github_token.txt')){ $tok=(Get-Content '%DEST%\github_token.txt' -Raw).Trim() };" ^
  "if($tok){ $h['Authorization']=('Bearer ' + $tok) };" ^
  "$r=Invoke-RestMethod -Headers $h -Uri 'https://api.github.com/repos/%REPO%/releases/latest';" ^
  "$a=$r.assets | Where-Object { $_.name -like 'tabex-windows-x64-*.zip' } | Select-Object -First 1;" ^
  "if(-not $a){ throw 'A release nao tem um pacote para Windows.' };" ^
  "$h['Accept']='application/octet-stream';" ^
  "Invoke-WebRequest -Headers $h -Uri $a.url -OutFile '%ZIP%';" ^
  "Write-Host ('  Baixado ' + $r.tag_name + ' (' + $a.name + ')')"
if errorlevel 1 (
    echo.
    echo  Nao foi possivel baixar a release. Se o repositorio for privado,
    echo  salve um token do GitHub em %DEST%\github_token.txt e rode de novo.
    goto :fail
)

:extract
echo  Extraindo para %DEST% ...
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference='Stop';" ^
  "Expand-Archive -LiteralPath '%ZIP%' -DestinationPath '%DEST%' -Force;" ^
  "Unblock-File -LiteralPath '%DEST%\tabex.exe'"
if errorlevel 1 goto :fail
if not exist "%DEST%\tabex.exe" (
    echo  O pacote nao contem tabex.exe.
    goto :fail
)

rem Modo portatil: configuracoes deste computador ficam em C:\tabex (session.json).
if not exist "%DEST%\portable.txt" echo As configuracoes do Tabex deste computador ficam nesta pasta.> "%DEST%\portable.txt"

rem Copia este instalador para C:\tabex (serve como "atualizar.bat").
if /I not "%~dp0"=="%DEST%\" (
    copy /Y "%~f0" "%DEST%\instalar.bat" >nul
    copy /Y "%~f0" "%DEST%\atualizar.bat" >nul
)

echo  Criando atalhos e adicionando ao PATH do usuario ...
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$s=New-Object -ComObject WScript.Shell;" ^
  "foreach($d in @([Environment]::GetFolderPath('Programs'),[Environment]::GetFolderPath('Desktop'))){" ^
  "  $l=$s.CreateShortcut((Join-Path $d 'Tabex.lnk'));" ^
  "  $l.TargetPath='%DEST%\tabex.exe'; $l.WorkingDirectory='%DEST%'; $l.IconLocation='%DEST%\tabex.exe,0';" ^
  "  $l.Description='Tabex - Explorador de arquivos com grupos de abas'; $l.Save() };" ^
  "$p=[Environment]::GetEnvironmentVariable('Path','User');" ^
  "if($p -notlike '*%DEST%*'){ [Environment]::SetEnvironmentVariable('Path', (($p.TrimEnd(';')) + ';%DEST%'), 'User') }"

echo.
echo  Pronto! Tabex instalado em %DEST%
echo  Atalhos: Menu Iniciar e Area de Trabalho.
echo  Para atualizar depois: rode %DEST%\atualizar.bat
echo  (ou use "... > Atualizacoes" dentro do Tabex)
echo.
start "" "%DEST%\tabex.exe"
exit /b 0

:fail
echo.
echo  A instalacao falhou.
pause
exit /b 1
