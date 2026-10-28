/**
 * 新建文件夹的名称校验，与后端 `guard::write::validate_folder_name` 同一套规则与文案。
 *
 * 远端是 POSIX：只拒绝真正危险或不可表示的名称（分隔符、NUL、`.`/`..`、超长），
 * 空格、前导点等合法名称一律放行，避免前端比后端更严。
 * 长度按 UTF-8 字节算（`NAME_MAX` 是字节上限，不是字符数）。
 */
export const MAX_NAME_BYTES = 255;

const encoder = new TextEncoder();

export function validateFolderName(name: string): string | null {
  const trimmed = name.trim();
  if (!trimmed) return '文件夹名称不能为空';
  if (encoder.encode(trimmed).length > MAX_NAME_BYTES) return '文件夹名称不能超过 255 字节';
  if (trimmed.includes('/') || trimmed.includes('\\') || trimmed.includes('\0')) {
    return '文件夹名称不能包含 / \\ 或 NUL';
  }
  if (trimmed === '.' || trimmed === '..') return '不能使用 . 或 .. 作为名称';
  return null;
}
