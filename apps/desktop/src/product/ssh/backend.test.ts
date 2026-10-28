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
});
