import { describe, expect, it } from 'vitest';
import { MAX_NAME_BYTES, validateFolderName } from './folderName';

describe('folder name', () => {
  it('rejects empty and reserved names', () => {
    expect(validateFolderName('')).not.toBeNull();
    expect(validateFolderName('   ')).not.toBeNull();
    expect(validateFolderName('.')).not.toBeNull();
    expect(validateFolderName('..')).not.toBeNull();
  });

  it('rejects separators and NUL', () => {
    expect(validateFolderName('a/b')).not.toBeNull();
    expect(validateFolderName('a\\b')).not.toBeNull();
    expect(validateFolderName('a\0b')).not.toBeNull();
  });

  it('rejects control characters the backend also rejects', () => {
    // 粘贴带 TAB / DEL 的名称：本地就要拦，不能等到预览才被后端拒绝。
    expect(validateFolderName('Q3\t2026')).toBe('文件夹名称不能包含控制字符');
    expect(validateFolderName('Q3\u007f2026')).toBe('文件夹名称不能包含控制字符');
  });

  it('counts bytes rather than characters', () => {
    expect(validateFolderName('汉'.repeat(85))).toBeNull();
    expect(validateFolderName('汉'.repeat(86))).not.toBeNull();
    expect(validateFolderName('  reports 2026  ')).toBeNull();
  });

  it('allows names the backend also accepts', () => {
    // 后端只拦分隔符、NUL、控制字符、`.`/`..` 与超长；空格与前导点都是合法名称。
    expect(validateFolderName('re ports')).toBeNull();
    expect(validateFolderName('Q3 2026')).toBeNull();
    expect(validateFolderName('.hidden')).toBeNull();
    expect(validateFolderName('a.b')).toBeNull();
  });

  it('names the byte limit the backend uses', () => {
    expect(MAX_NAME_BYTES).toBe(255);
  });
});
