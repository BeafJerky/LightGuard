import { describe, expect, it } from 'vitest';
import { toSshError } from './backend';

describe('file error mapping', () => {
  it('keeps stable file codes', () => {
    for (const code of [
      'SFTP_FAILED',
      'PATH_INVALID',
      'FILE_NOT_FOUND',
      'PERMISSION_DENIED',
      'FILE_TOO_LARGE',
      'FILE_BINARY',
      'TRANSFER_FAILED',
    ] as const) {
      expect(toSshError({ code, message: 'x', hint: 'y' }).code).toBe(code);
    }
  });

  it('keeps sftp and transfer stages', () => {
    expect(toSshError({ code: 'SFTP_FAILED', message: 'x', stage: 'sftp' }).stage).toBe('sftp');
    expect(toSshError({ code: 'TRANSFER_FAILED', message: 'x', stage: 'transfer' }).stage).toBe(
      'transfer',
    );
  });

  it('keeps guard and alert codes with hints', () => {
    for (const code of [
      'PERMIT_REQUIRED',
      'WRITE_BUSY',
      'PREVIEW_EXPIRED',
      'PREVIEW_FAILED',
      'WRITE_FAILED',
      'DB_UNAVAILABLE',
    ] as const) {
      const error = toSshError({ code, message: 'x', hint: 'y' });
      expect(error.code).toBe(code);
      expect(error.hint).toBe('y');
    }
  });
});
