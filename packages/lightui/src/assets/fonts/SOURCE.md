# LG Pixel 字体来源

- 上游字体：Fusion Pixel 12px Monospaced SC（作者 TakWolf，https://github.com/TakWolf/fusion-pixel-font）
- 上游版本：fontsource 包 `@fontsource/fusion-pixel-12px-monospaced-sc@5.3.0`
- 许可：SIL Open Font License 1.1（见 `OFL.txt`）；上游保留字体名 `Fusion Pixel`
- 本文件：`lg-pixel-12px-subset.woff2`
- 修改：按 LightGuard 原型实际字符集做子集化，并将字体族名改为 `LG Pixel`
- 原因：上游保留字体名不允许修改后的子集继续使用原名；`LG Pixel` 仅在本原型内使用
- 字符范围：ASCII 可见字符、原型中出现的中日韩统一表意文字、常用中文标点与箭头
- 再生成：
  1. `npm pack @fontsource/fusion-pixel-12px-monospaced-sc@5.3.0 --pack-destination /tmp/lg-font`
  2. 解包后取 `package/files/fusion-pixel-12px-monospaced-sc-latin-400-normal.woff2` 作为源文件
  3. `python tools/build-pixel-font.py <源文件路径>`（默认路径为 `/tmp/lg-font/source.woff2`；Windows 下 `/tmp` 映射到当前盘符的 `\tmp`）
