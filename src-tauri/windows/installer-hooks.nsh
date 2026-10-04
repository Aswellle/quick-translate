; src-tauri/windows/installer-hooks.nsh
; Custom NSIS installer hooks, wired via bundle.windows.nsis.installerHooks
; in tauri.conf.json. Only the macros actually defined below are inserted by
; the Tauri template; registers $R8/$R9 carry state between uninstall hooks
; and are not touched by the template in between.
;
; PREINSTALL   keep every installed file inside a folder named after the
;              product, even when a custom directory is picked on the folder
;              page; an existing install is upgraded in place.
; PREUNINSTALL stop the running app silently (a tray app keeps the binary
;              locked; the template's prompt can be cancelled mid-uninstall)
;              and remember whether $INSTDIR really is the app directory.
; POSTUNINSTALL remove autostart entries written under names the template
;              does not know about, then sweep the install directory.

!macro NSIS_HOOK_PREINSTALL
  ${If} $INSTDIR != ""
    ; Normalize a trailing backslash so GetFileName sees the folder name.
    StrCpy $R9 "$INSTDIR" "" -1
    ${If} $R9 == "\"
      StrCpy $INSTDIR "$INSTDIR" -1
    ${EndIf}
    ${GetFileName} $INSTDIR $R9
    ${If} $R9 != "${PRODUCTNAME}"
      ; Fresh directory picked by the user: append the product folder so files
      ; never land loose in the chosen path. An existing install (the binary is
      ; already there) keeps its directory and upgrades in place.
      ${IfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
        StrCpy $INSTDIR "$INSTDIR\${PRODUCTNAME}"
      ${EndIf}
    ${EndIf}
    ; The template pointed its extraction dir at the old $INSTDIR before this
    ; hook ran — repoint it so File writes follow the corrected directory.
    SetOutPath $INSTDIR
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Kill the whole process tree up front: the tray app keeps the main binary
  ; locked for as long as it runs, which is the usual reason an uninstall
  ; leaves the program behind. The template's prompt-based close runs after
  ; this hook and finds nothing to ask about.
  nsExec::Exec 'taskkill /F /T /IM "${MAINBINARYNAME}.exe"'
  Pop $R0
  Sleep 300

  ; Mark that $INSTDIR really is the app directory (binary present, or the
  ; product-named folder the installer owns) so POSTUNINSTALL can sweep it
  ; without ever recursing into an arbitrary path.
  ${GetFileName} $INSTDIR $R9
  ${If} $R9 == "${PRODUCTNAME}"
    StrCpy $R8 1
  ${ElseIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R8 1
  ${Else}
    StrCpy $R8 0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Autostart entries are written at runtime under the Run key using the
  ; binary name; the template only removes "${PRODUCTNAME}". Clear the other
  ; names too, plus Explorer's StartupApproved store, or the app resurrects
  ; at next login after an uninstall. Updates keep the user's setting.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${MAINBINARYNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${BUNDLEID}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${MAINBINARYNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${BUNDLEID}"
  ${EndIf}

  ; The template deletes only the files it installed and then runs a plain
  ; RMDir — any file that appears next to the app afterwards blocks it and
  ; leaves the whole directory behind as residue. Finish with a real sweep.
  ${If} $R8 = 1
    RmDir /r "$INSTDIR"
  ${EndIf}
!macroend
