import { describe, expect, it, vi } from 'vitest';
import { UpdateController, classifyUpdateError } from './controller';
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

  it('clears stale update info when an automatic check fails', async () => {
    const check = vi
      .fn()
      .mockResolvedValueOnce(info)
      .mockRejectedValueOnce(new Error('network unreachable'));
    const controller = new UpdateController(bridge({ check }));
    await controller.check({ manual: true });
    expect(controller.state.info?.version).toBe('0.14.0');
    await controller.check({ manual: false });
    expect(controller.state.phase).toBe('idle');
    expect(controller.state.info).toBeNull();
    expect(controller.state.failedAction).toBeNull();
  });

  it('shows a classified error for manual check failures', async () => {
    const controller = new UpdateController(
      bridge({ check: vi.fn(async () => { throw new Error('request timeout'); }) }),
    );
    await controller.check({ manual: true });
    expect(controller.state.phase).toBe('error');
    expect(controller.state.error?.kind).toBe('network');
    expect(controller.state.failedAction).toBe('check');
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

  it('records a download failure so retry targets the download step', async () => {
    const controller = new UpdateController(
      bridge({
        download: vi.fn(async () => { throw new Error('write file failed: disk full'); }),
      }),
    );
    await controller.check({ manual: true });
    await controller.download();
    expect(controller.state.phase).toBe('error');
    expect(controller.state.failedAction).toBe('download');
  });

  it('records an install failure and allows retrying the install', async () => {
    const install = vi
      .fn()
      .mockRejectedValueOnce(new Error('installer failed'))
      .mockResolvedValueOnce(undefined);
    const controller = new UpdateController(bridge({ install }));
    await controller.check({ manual: true });
    await controller.download();
    expect(await controller.install()).toBe(false);
    expect(controller.state.phase).toBe('error');
    expect(controller.state.failedAction).toBe('install');
    expect(await controller.install()).toBe(true);
    expect(install).toHaveBeenCalledTimes(2);
  });

  it('ignores a second install while one is in flight', async () => {
    let release: () => void = () => {};
    const install = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const controller = new UpdateController(bridge({ install }));
    await controller.check({ manual: true });
    await controller.download();
    const first = controller.install();
    expect(await controller.install()).toBe(false);
    release();
    expect(await first).toBe(true);
    expect(install).toHaveBeenCalledOnce();
  });

  it('reports a relaunch failure and keeps the relaunch as the retry target', () => {
    const controller = new UpdateController(bridge());
    controller.reportRelaunchFailure();
    expect(controller.state.phase).toBe('error');
    expect(controller.state.failedAction).toBe('relaunch');
    expect(controller.state.error?.message).toContain('重新打开 LightGuard');
  });

  it('restores a cached package into ready without downloading', async () => {
    const bridge: UpdaterBridge = {
      check: vi.fn(async () => ({
        version: '0.15.0',
        currentVersion: '0.14.1',
        cached: true,
        size: 1024,
      })),
      download: vi.fn(),
      install: vi.fn(),
    };
    const controller = new UpdateController(bridge);
    await controller.check({ manual: true });
    expect(controller.state.phase).toBe('ready');
    expect(controller.state.progress).toEqual({ downloaded: 1024, total: 1024 });
    expect(bridge.download).not.toHaveBeenCalled();
  });

  it('cancels a download and returns to available', async () => {
    let rejectDownload: (error: Error) => void = () => {};
    const bridge: UpdaterBridge = {
      check: vi.fn(async () => ({ version: '0.15.0', currentVersion: '0.14.1' })),
      download: vi.fn(
        () =>
          new Promise<void>((_, reject) => {
            rejectDownload = reject;
          }),
      ),
      install: vi.fn(),
      cancel: vi.fn(async () => {
        rejectDownload(new Error('cancelled'));
      }),
    };
    const controller = new UpdateController(bridge);
    await controller.check({ manual: true });
    const downloading = controller.download();
    await Promise.resolve();
    await controller.cancel();
    await downloading;
    expect(controller.state.phase).toBe('available');
    expect(controller.state.error).toBeNull();
  });

  it('keeps the ready state after defer', async () => {
    const bridge: UpdaterBridge = {
      check: vi.fn(async () => ({ version: '0.15.0', currentVersion: '0.14.1' })),
      download: vi.fn(async (_info, onProgress) => onProgress({ downloaded: 10, total: 10 })),
      install: vi.fn(),
      defer: vi.fn(async () => {}),
    };
    const controller = new UpdateController(bridge);
    await controller.check({ manual: true });
    await controller.download();
    await controller.defer();
    expect(controller.state.phase).toBe('ready');
    expect(bridge.defer).toHaveBeenCalledTimes(1);
  });

  it('classifies signatures before network and manifest errors', () => {
    expect(classifyUpdateError(new Error('invalid signature for update'))).toBe('signature');
    expect(classifyUpdateError(new Error('failed to deserialize update response'))).toBe('manifest');
    expect(classifyUpdateError(new Error('write file failed: disk full'))).toBe('disk');
    expect(classifyUpdateError(new Error('request timeout'))).toBe('network');
    expect(classifyUpdateError(new Error('something else'))).toBe('unknown');
  });
});
