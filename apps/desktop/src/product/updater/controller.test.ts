import { describe, expect, it, vi } from 'vitest';
import {
  MAX_BUFFERED_BYTES,
  UpdateController,
  classifyUpdateError,
} from './controller';
import type { DownloadProgress, UpdateInfo, UpdaterBridge } from './types';

const info: UpdateInfo = {
  version: '0.14.0',
  currentVersion: '0.13.0',
  date: '2026-11-02T00:00:00Z',
  body: 'release notes',
  size: 1024,
  url: 'https://example.com/LightGuard.nsis.zip',
};

function bridge(overrides: Partial<UpdaterBridge> = {}): UpdaterBridge {
  return {
    check: vi.fn(async () => info),
    download: vi.fn(async (_info, onProgress: (progress: DownloadProgress) => void) => {
      onProgress({ downloaded: 512, total: 1024 });
    }),
    install: vi.fn(async () => {}),
    ...overrides,
  };
}

describe('update controller', () => {
  it('moves from checking to available when a version is found', async () => {
    const controller = new UpdateController(bridge());
    await controller.check({ manual: true });
    expect(controller.state.phase).toBe('available');
    expect(controller.state.info?.version).toBe('0.14.0');
  });

  it('reports up-to-date when the plugin returns null', async () => {
    const controller = new UpdateController(bridge({ check: vi.fn(async () => null) }));
    await controller.check({ manual: true });
    expect(controller.state.phase).toBe('up-to-date');
    expect(controller.state.info).toBeNull();
  });

  it('keeps automatic check failures silent', async () => {
    const controller = new UpdateController(
      bridge({ check: vi.fn(async () => { throw new Error('network unreachable'); }) }),
    );
    await controller.check({ manual: false });
    expect(controller.state.phase).toBe('idle');
    expect(controller.state.error).toBeNull();
  });

  it('shows a classified error for manual check failures', async () => {
    const controller = new UpdateController(
      bridge({ check: vi.fn(async () => { throw new Error('request timeout'); }) }),
    );
    await controller.check({ manual: true });
    expect(controller.state.phase).toBe('error');
    expect(controller.state.error?.kind).toBe('network');
  });

  it('downloads into ready and forwards progress', async () => {
    const controller = new UpdateController(bridge());
    await controller.check({ manual: true });
    await controller.download();
    expect(controller.state.phase).toBe('ready');
    expect(controller.state.progress).toEqual({ downloaded: 512, total: 1024 });
  });

  it('installs only after a completed download', async () => {
    const install = vi.fn(async () => {});
    const controller = new UpdateController(bridge({ install }));
    await controller.check({ manual: true });
    expect(await controller.install()).toBe(false);
    await controller.download();
    expect(await controller.install()).toBe(true);
    expect(install).toHaveBeenCalledWith(info);
    expect(controller.state.phase).toBe('installing');
  });

  it('uses downloadAndInstall for an oversized package', async () => {
    const download = vi.fn(async () => {});
    const downloadAndInstall = vi.fn(async () => {});
    const controller = new UpdateController(
      bridge({
        check: vi.fn(async () => ({ ...info, size: MAX_BUFFERED_BYTES + 1 })),
        download,
        downloadAndInstall,
      }),
    );
    await controller.check({ manual: true });
    await controller.download();
    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(download).not.toHaveBeenCalled();
    expect(controller.state.phase).toBe('installing');
  });

  it('classifies signatures before network and manifest errors', () => {
    expect(classifyUpdateError(new Error('invalid signature for update'))).toBe('signature');
    expect(classifyUpdateError(new Error('failed to deserialize update response'))).toBe('manifest');
    expect(classifyUpdateError(new Error('write file failed: disk full'))).toBe('disk');
    expect(classifyUpdateError(new Error('request timeout'))).toBe('network');
    expect(classifyUpdateError(new Error('something else'))).toBe('unknown');
  });
});
