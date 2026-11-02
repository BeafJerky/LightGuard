; 仅在尚未记住 Installer Language 时显示语言选择框：首次安装会询问，
; 自动更新（passive）与重装沿用 HKCU\Software\LightGuard\LightGuard 中的选择，不再弹框。
; 不要定义 MUI_LANGDLL_ALWAYSSHOW，否则更新也会被语言框挡住。
!define MUI_LANGDLL_WINDOWTITLE "LightGuard 安装语言 / Setup Language"
!define MUI_LANGDLL_INFO "请选择安装界面语言。 Please select a language."
