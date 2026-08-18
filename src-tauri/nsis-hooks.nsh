; VibeShell NSIS installer hooks
; Adds/removes the install directory to/from user PATH so that
; `vibeshell` CLI is available from new terminals after installation.

!macro NSIS_HOOK_PREINSTALL
  ; Stop the native daemon/CLI before replacing the bundled sidecar.
  nsExec::ExecToLog 'taskkill.exe /F /IM vibeshell.exe'
  Delete "$INSTDIR\vibeshell.exe"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Add the install directory exactly once to the current user's PATH.
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$$entries = [Environment]::GetEnvironmentVariable(''Path'',''User'') -split '';''; if (-not ($$entries | Where-Object { $$_ -ieq ''$INSTDIR'' })) { $$updated = @($$entries | Where-Object { $$_ -ne '''' }) + ''$INSTDIR''; [Environment]::SetEnvironmentVariable(''Path'', ($$updated -join '';''), ''User'') }"'
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'taskkill.exe /F /IM vibeshell.exe'
  ; Read current user PATH
  ReadRegStr $0 HKCU "Environment" "Path"
  StrCmp $0 "" _vs_unpath_done
    ; Use nsExec + PowerShell to cleanly remove $INSTDIR from PATH
    nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$$entries = [Environment]::GetEnvironmentVariable(''Path'',''User'') -split '';''; $$filtered = $$entries | Where-Object { $$_ -ne ''$INSTDIR'' -and $$_ -ne '''' }; [Environment]::SetEnvironmentVariable(''Path'', ($$filtered -join '';''), ''User'')"'
  _vs_unpath_done:
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend
