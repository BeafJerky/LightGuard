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

  it('counts bytes rather than characters', () => {
    expect(validateFolderName('汉'.repeat(85))).toBeNull();
    expect(validateFolderName('汉'.repeat(86))).not.toBeNull();
    expect(validateFolderName('  reports 2026  ')).toBeNull();
  });

  it('allows names the backend also accepts', () => {
    // 后端只拦分隔符、NUL、`.`/`..` 与超长；空格与前导点都是合法名称。
    expect(validateFolderName('re ports')).toBeNull();
    expect(validateFolderName('.hidden')).toBeNull();
    expect(validateFolderName('a.b')).toBeNull();
  });

  it('names the byte limit the backend uses', () => {
    expect(MAX_NAME_BYTES).toBe(255);
  });
});
