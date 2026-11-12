; PureMinecraft 安装向导（NSIS）
Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define APP_NAME      "PureMinecraft"
!define APP_VER       "1.0.0"
!define APP_PUBLISHER "PureStat Lab"
!define APP_KEY       "Software\Microsoft\Windows\CurrentVersion\Uninstall\PureMinecraft"

Name "${APP_NAME}"
BrandingText "${APP_NAME} ${APP_VER}"
OutFile "..\dist\PureMinecraft-Setup-${APP_VER}.exe"

; 不写死盘符，跟随系统用户目录
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
InstallDirRegKey HKCU "${APP_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32

VIProductVersion "1.0.0.0"
VIAddVersionKey /LANG=2052 "ProductName"     "${APP_NAME}"
VIAddVersionKey /LANG=2052 "FileDescription" "${APP_NAME} 安装程序"
VIAddVersionKey /LANG=2052 "FileVersion"     "${APP_VER}.0"
VIAddVersionKey /LANG=2052 "ProductVersion"  "${APP_VER}.0"
VIAddVersionKey /LANG=2052 "CompanyName"     "${APP_PUBLISHER}"
VIAddVersionKey /LANG=2052 "LegalCopyright"  "Copyright (C) 2026"

!define MUI_ICON   "..\resources\PureMinecraft.ico"
!define MUI_UNICON "..\resources\PureMinecraft.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\bin\${APP_NAME}.exe"
!define MUI_FINISHPAGE_RUN_TEXT "立即运行 PureMinecraft"
!define MUI_WELCOMEPAGE_TITLE "${APP_NAME} 安装向导"
!define MUI_WELCOMEPAGE_TEXT  "将安装 ${APP_NAME} ${APP_VER}。$\r$\n$\r$\n极简 Minecraft 启动器（Fabric 1.20.1）：版本下载、模组库、小地图。$\r$\n$\r$\n安装位置默认在用户目录下，不写死盘符；游戏目录 .minecraft 与 Java 运行时会自动在安装目录里查找，整个文件夹可以拷到任何盘符/电脑直接使用。"
!define MUI_FINISHPAGE_TITLE  "${APP_NAME} 安装完成"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "PureMinecraft" SecMain
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File /r "..\stage\*.*"

  CreateShortCut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\bin\${APP_NAME}.exe" "" "$INSTDIR\bin\${APP_NAME}.exe" 0
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\bin\${APP_NAME}.exe" "" "$INSTDIR\bin\${APP_NAME}.exe" 0
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\卸载 ${APP_NAME}.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${APP_KEY}" "DisplayName"     "${APP_NAME}"
  WriteRegStr HKCU "${APP_KEY}" "DisplayVersion"  "${APP_VER}"
  WriteRegStr HKCU "${APP_KEY}" "Publisher"       "${APP_PUBLISHER}"
  WriteRegStr HKCU "${APP_KEY}" "DisplayIcon"     "$INSTDIR\bin\${APP_NAME}.exe"
  WriteRegStr HKCU "${APP_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${APP_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${APP_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${APP_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${APP_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${APP_KEY}" "EstimatedSize" "$0"
SectionEnd
