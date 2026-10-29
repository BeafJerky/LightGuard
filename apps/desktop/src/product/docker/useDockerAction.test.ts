import { describe, expect, it, vi } from 'vitest';
import type { DockerApi, DockerPreview, DockerWriteResult } from './api';
import { useDockerSystem, useDockerWrite } from './useDockerAction';

function preview(patch: Partial<DockerPreview> = {}): DockerPreview {
  return {
    previewId: 'p1',
    kind: 'container.stop',
    operation: '停止容器',
    target: 'shop-web-1',
    impact: '容器将停止',
    rollback: '可重新启动',
    irreversible: false,
    expiresAtMs: 1_000,
    details: [],
    ...patch,
  };
}

function result(patch: Partial<DockerWriteResult> = {}): DockerWriteResult {
  return { auditId: 7, target: 'shop-web-1', result: 'success', message: '已完成', ...patch };
}

function api(overrides: Partial<DockerApi> = {}): DockerApi {
  return {
    available: true,
    capabilities: vi.fn(),
    overview: vi.fn(),
    containers: vi.fn(),
    containerDetail: vi.fn(),
    stats: vi.fn(),
    images: vi.fn(),
    registryConfig: vi.fn(),
    registrySearch: vi.fn(),
    writePreview: vi.fn(),
    writeExecute: vi.fn(),
    writeCancel: vi.fn(),
    systemPreview: vi.fn(),
    systemExecute: vi.fn(),
    sudoStatus: vi.fn(),
    openContainerTerminal: vi.fn(),
    ...overrides,
  } as unknown as DockerApi;
}

describe('use docker write', () => {
  it('marks the preview spent so a confirmed write cannot run twice', async () => {
    const fake = api();
    vi.mocked(fake.writePreview).mockResolvedValue(preview());
    vi.mocked(fake.writeExecute).mockResolvedValue(result());
    const hook = useDockerWrite({ id: 'srv_1' } as never, fake);

    expect(await hook.ask({ kind: 'container.stop', target: 'shop-web-1' })).toBe(true);
    expect(await hook.confirm()).toMatchObject({ result: 'success' });
    expect(hook.spent.value).toBe(true);

    expect(await hook.confirm()).toBeNull();
    expect(fake.writeExecute).toHaveBeenCalledTimes(1);
  });

  it('re-previews once when the preview expired and executes the fresh one', async () => {
    const fake = api();
    vi.mocked(fake.writePreview)
      .mockResolvedValueOnce(preview({ previewId: 'p1' }))
      .mockResolvedValueOnce(preview({ previewId: 'p2' }));
    vi.mocked(fake.writeExecute)
      .mockRejectedValueOnce({ code: 'PREVIEW_EXPIRED', message: '预览已过期', hint: '' })
      .mockResolvedValueOnce(result());
    const hook = useDockerWrite({ id: 'srv_1' } as never, fake);

    expect(await hook.ask({ kind: 'container.stop', target: 'shop-web-1' })).toBe(true);
    expect(await hook.confirm()).toBeNull();
    expect(hook.preview.value?.previewId).toBe('p2');
    expect(hook.spent.value).toBe(false);
    expect(hook.error.value).toContain('已重新生成');
    expect(hook.progress.value).toEqual([]);

    expect(await hook.confirm()).toMatchObject({ result: 'success' });
    expect(fake.writeExecute).toHaveBeenLastCalledWith('p2', expect.any(Function));
  });
});

describe('use docker system', () => {
  it('opens the sudo dialog when a system preview needs authorization', async () => {
    const fake = api();
    vi.mocked(fake.systemPreview).mockRejectedValue({
      code: 'SUDO_REQUIRED',
      message: '需要 sudo 密码',
      hint: '',
    });
    const hook = useDockerSystem({ id: 'srv_1' } as never, fake);

    expect(await hook.ask({ kind: 'service.stop', payload: {} })).toBe(false);
    expect(hook.authOpen.value).toBe(true);
  });

  it('closes the sudo dialog after three failed passwords', async () => {
    const fake = api();
    vi.mocked(fake.systemPreview).mockRejectedValue({
      code: 'SUDO_FAILED',
      message: '密码错误',
      hint: '',
    });
    const hook = useDockerSystem({ id: 'srv_1' } as never, fake);

    // 首次预览失败时还没有缓存密码，不计入连续失败次数。
    expect(await hook.ask({ kind: 'service.stop', payload: {} })).toBe(false);
    expect(hook.authOpen.value).toBe(true);
    expect(hook.failures.value).toBe(0);

    await hook.submit('bad');
    await hook.submit('bad');
    await hook.submit('bad');

    expect(hook.failures.value).toBe(3);
    expect(hook.authOpen.value).toBe(false);
    expect(hook.error.value).toContain('连续 3 次');
  });

  it('re-previews when authorization lapses at confirm time', async () => {
    const fake = api();
    vi.mocked(fake.systemPreview)
      .mockResolvedValueOnce(preview({ previewId: 'p1' }))
      .mockRejectedValueOnce({ code: 'SUDO_REQUIRED', message: '需要 sudo 密码', hint: '' });
    vi.mocked(fake.systemExecute).mockRejectedValue({
      code: 'SUDO_REQUIRED',
      message: '需要 sudo 密码',
      hint: '',
    });
    const hook = useDockerSystem({ id: 'srv_1' } as never, fake);

    expect(await hook.ask({ kind: 'service.stop', payload: {} })).toBe(true);
    expect(await hook.confirm()).toBeNull();
    expect(hook.preview.value).toBeNull();
    expect(hook.authOpen.value).toBe(true);
    expect(hook.spent.value).toBe(false);
  });
});
