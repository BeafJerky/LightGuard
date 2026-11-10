; 仅在尚未记住 Installer Language 时显示语言选择框：首次安装会询问，
; 自动更新（passive）与重装沿用 HKCU\Software\LightGuard\LightGuard 中的选择，不再弹框。
; 不要定义 MUI_LANGDLL_ALWAYSSHOW，否则更新也会被语言框挡住。
!define MUI_LANGDLL_WINDOWTITLE "LightGuard 安装语言 / Setup Language"
!define MUI_LANGDLL_INFO "请选择安装界面语言。 Please select a language."

; 图标已换但安装路径不变时，资源管理器会沿用按路径缓存的旧图标，任务栏继续显示旧盾牌。
; 安装末尾按文件标记缓存失效，升级用户下次启动即可看到新图标。
!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x00002000, i 0x0005, w "$INSTDIR\${MAINBINARYNAME}.exe", p 0)'
!macroend
