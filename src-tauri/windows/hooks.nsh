; Перед установкой закрываем работающий инфокиоск и его встроенный
; WebView2: иначе установщик не может перезаписать файлы WebView2
; («Error opening file for writing … d3dcompiler_47.dll / ru.pak»).
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /T /IM infokiosk.exe'
  Pop $0
  nsExec::Exec `wmic process where "name='msedgewebview2.exe' and ExecutablePath like '%FixedVersionRuntime.109%'" call terminate`
  Pop $0
  Sleep 2000
!macroend
