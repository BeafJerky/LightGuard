; 每次安装/卸载都显示语言选择框。
; 不定义时 NSIS 会沿用 HKCU\Software\LightGuard\LightGuard 里记住的 Installer Language，
; 从只带英文的旧版本升级的用户将永远看不到选择框。
!define MUI_LANGDLL_ALWAYSSHOW
!define MUI_LANGDLL_WINDOWTITLE "LightGuard 安装语言 / Setup Language"
!define MUI_LANGDLL_INFO "请选择安装界面语言。 Please select a language."
