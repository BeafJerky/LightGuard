import { describe, expect, it, vi } from 'vitest';
import type { DockerApi, DockerCapabilities, DockerOverview } from './api';
import { useDocker } from './useDocker';

function capabilities(patch: Partial<DockerCapabilities> = {}): DockerCapabilities {
  return {
    installed: true,
    daemonRunning: true,
    socketOk: true,
    systemd: true,
    autoStart: 'enabled',
    compose: true,
    version: '27.0.0',
    apiVersion: '1.46',
    proxySupported: true,
    message: '',
    ...patch,
  };
}

function overview(): DockerOverview {
  return {
    capabilities: capabilities(),
    info: {
      version: '27.0.0',
      apiVersion: '1.46',
      storageDriver: 'overlay2',
      dataRoot: '/var/lib/docker',
      mirrors: ['https://mirror.example.com'],
    },
    counts: { containersRunning: 3, containersTotal: 5, images: 9 },
    df: null,
  };
}

function api(overrides: Partial<DockerApi> = {}): DockerApi {
  return {
    available: true,
    capabilities: vi.fn(async () => capabilities()),
    overview: vi.fn(async () => overview()),
    containers: vi.fn(async () => []),
    containerDetail: vi.fn(),
    stats: vi.fn(async () => []),
    images: vi.fn(async () => []),
    registryConfig: vi.fn(),
    registrySearch: vi.fn(async () => []),
    writePreview: vi.fn(),
    writeExecute: vi.fn(),
    writeCancel: vi.fn(async () => false),
    systemPreview: vi.fn(),
    systemExecute: vi.fn(),
    sudoStatus: vi.fn(),
    openContainerTerminal: vi.fn(),
    ...overrides,
  } as DockerApi;
}

describe('use docker', () => {
  it('loads the overview only when the daemon runs', async () => {
    const docker = useDocker({ id: 'srv_1' } as never, api());
    await docker.load();
    expect(docker.installed.value).toBe(true);
    expect(docker.running.value).toBe(true);
    expect(docker.overview.value?.counts.images).toBe(9);
    expect(docker.loading.value).toBe(false);
  });

  it('keeps the service state when docker is stopped', async () => {
    const stopped = api({
      capabilities: vi.fn(async () => capabilities({ daemonRunning: false, message: 'Docker 服务未运行' })),
    });
    const docker = useDocker({ id: 'srv_1' } as never, stopped);
    await docker.load();
    expect(docker.running.value).toBe(false);
    expect(docker.overview.value).toBeNull();
    expect(stopped.overview).not.toHaveBeenCalled();
  });

  it('keeps the last overview when a silent refresh fails', async () => {
    const flaky = api();
    const docker = useDocker({ id: 'srv_1' } as never, flaky);
    await docker.load();
    vi.mocked(flaky.capabilities).mockRejectedValueOnce({
      code: 'DOCKER_FAILED',
      message: '读取超时',
      hint: '',
    });
    await docker.load(false, true);
    expect(docker.error.value).toBe('');
    expect(docker.overview.value?.counts.images).toBe(9);
  });

  it('surfaces command errors', async () => {
    const failing = api({
      capabilities: vi.fn(async () => {
        throw { code: 'DOCKER_UNAVAILABLE', message: '未安装 Docker', hint: '' };
      }),
    });
    const docker = useDocker({ id: 'srv_1' } as never, failing);
    await docker.load();
    expect(docker.error.value).toBe('未安装 Docker');
  });

  it('recovers from an error after a later silent load succeeds', async () => {
    const flaky = api();
    const docker = useDocker({ id: 'srv_1' } as never, flaky);
    vi.mocked(flaky.capabilities).mockRejectedValueOnce({
      code: 'DOCKER_FAILED',
      message: '读取超时',
      hint: '',
    });
    await docker.load();
    expect(docker.error.value).toBe('读取超时');
    expect(docker.overview.value).toBeNull();

    await docker.load(true, true);
    expect(docker.error.value).toBe('');
    expect(docker.overview.value?.counts.images).toBe(9);
  });

  it('a failing non-silent load clears a populated overview', async () => {
    const flaky = api();
    const docker = useDocker({ id: 'srv_1' } as never, flaky);
    await docker.load();
    expect(docker.overview.value?.counts.images).toBe(9);

    vi.mocked(flaky.capabilities).mockRejectedValueOnce({
      code: 'DOCKER_FAILED',
      message: '读取超时',
      hint: '',
    });
    await docker.load();
    expect(docker.error.value).toBe('读取超时');
    expect(docker.overview.value).toBeNull();
  });

  it('skips the overview when docker is not installed', async () => {
    const missing = api({
      capabilities: vi.fn(async () => capabilities({ installed: false, daemonRunning: false })),
    });
    const docker = useDocker({ id: 'srv_1' } as never, missing);
    await docker.load();
    expect(docker.installed.value).toBe(false);
    expect(docker.overview.value).toBeNull();
    expect(missing.overview).not.toHaveBeenCalled();
  });
});
