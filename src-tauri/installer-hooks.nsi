; supertonic-reader uninstaller hooks.
;
; The Tauri NSIS template renders a "Delete application data" checkbox on the
; uninstall confirmation page and stores its state in $DeleteAppDataCheckboxState
; (1 = checked, 0 = unchecked). The template's default cleanup only touches
; %APPDATA%/%LOCALAPPDATA%\<bundle-id>, which we don't use — our models and
; settings live next to the exe at $INSTDIR\supertonic-reader-data\.
;
; So we read that same checkbox: if the user ticked it, we remove the data
; directory; otherwise we leave it alone so a re-install can pick it up.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
    IfFileExists "$INSTDIR\supertonic-reader-data\*.*" 0 spr_no_data_dir
      DetailPrint "Removing $INSTDIR\supertonic-reader-data ..."
      RMDir /r "$INSTDIR\supertonic-reader-data"
    spr_no_data_dir:
  ${EndIf}
!macroend
