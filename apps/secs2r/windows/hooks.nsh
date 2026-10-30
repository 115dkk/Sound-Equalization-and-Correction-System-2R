!include "LogicLib.nsh"

!define SECS2R_HOST_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define SECS2R_HOST_RUN_VALUE "SECS2RHostShell"
!define SECS2R_HOST_SERVICE_KEY "SYSTEM\CurrentControlSet\Services\SECS2RHost"

!macro EndSecs2RHostShell
  !insertmacro CheckIfAppIsRunning "$INSTDIR\secs2r-host-shell.exe" "SECS 2R Host"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro EndSecs2RHostShell
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ClearErrors
  ReadRegStr $0 HKCU "${SECS2R_HOST_RUN_KEY}" "${SECS2R_HOST_RUN_VALUE}"
  ${IfNot} ${Errors}
    Exec '"$INSTDIR\secs2r-host-shell.exe" --hidden'
  ${EndIf}
!macroend

; An update (the new installer removing the old files with /UPDATE) keeps the
; service and the logon start; only a real uninstall removes them, and only
; then does Windows ask for administrator approval.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro EndSecs2RHostShell

  ${If} $UpdateMode <> 1
    ClearErrors
    ReadRegStr $0 HKLM "${SECS2R_HOST_SERVICE_KEY}" "ImagePath"
    ${IfNot} ${Errors}
      ExecShellWait "runas" "$INSTDIR\secs2r-host.exe" "uninstall" SW_HIDE
    ${EndIf}
    DeleteRegValue HKCU "${SECS2R_HOST_RUN_KEY}" "${SECS2R_HOST_RUN_VALUE}"
  ${EndIf}
!macroend
