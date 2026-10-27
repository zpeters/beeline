; beeline per-user installer, built by `just installer-win`.
; Installs for the current user only, so no administrator rights are needed.
; The version comes from the command line: makensis /DVERSION=2.1.1

!ifndef VERSION
  !error "Pass the version: makensis /DVERSION=x.y.z beeline.nsi"
!endif

!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\beeline"

Unicode true
Name "beeline ${VERSION}"
OutFile "..\..\dist\beeline-${VERSION}-windows-x86_64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\beeline"
RequestExecutionLevel user
SetCompressor /SOLID lzma

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "beeline"
  SetOutPath "$INSTDIR"
  File "..\..\target\release\beeline.exe"
  File "..\..\README.md"
  File "..\..\LICENSE"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\beeline.lnk" "$INSTDIR\beeline.exe" "" "$INSTDIR\beeline.exe" 0 SW_SHOWNORMAL "" "beeline"

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "beeline"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\beeline.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\beeline.lnk"
  Delete "$INSTDIR\beeline.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
