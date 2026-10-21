; Extra steps for the NSIS installer Tauri generates (bundle.windows.nsis.installerHooks in tauri.conf.json).
;
; Bonsai writes two things to the registry itself, so the uninstaller has to remove them:
; - the Explorer right-click menu (src-tauri/src/shell_menu.rs), and
; - the "Start with Windows" entry (tauri-plugin-autostart, value name = product name).
; The extension list must match VIDEO_EXTENSIONS in shell_menu.rs (a unit test there checks it).

!macro NSIS_HOOK_POSTUNINSTALL
  ; An update runs the old uninstaller with /UPDATE: keep everything, the new version takes it over.
  ${If} $UpdateMode <> 1
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mp4\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mov\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.mkv\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.webm\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.avi\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.m4v\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.wmv\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.flv\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.ts\shell\Bonsai"
    DeleteRegKey HKCU "Software\Classes\Bonsai.Compress"

    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Bonsai"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Bonsai"
  ${EndIf}
!macroend
