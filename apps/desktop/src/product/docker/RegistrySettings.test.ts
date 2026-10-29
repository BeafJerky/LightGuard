// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';

const mocks = vi.hoisted(() => ({
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
}));

// 组件与 hook 都在模块作用域取 dockerApi：这里整体替换成假实现。
vi.mock('./api', () => ({
  dockerApi: { available: true, ...mocks },
}));

vi.mock('../ssh', () => ({
  ssh: {},
  toSshError: (error: unknown) => error,
}));

import type { RegistryConfig } from './api';
import type { Server } from '../types';
import RegistrySettings from './RegistrySettings.vue';

const server = { id: 'srv_1' } as Server;

function config(overrides: Partial<RegistryConfig> = {}): RegistryConfig {
  return {
    mirrors: ['https://docker.mirrors.ustc.edu.cn'],
    httpProxy: '',
    httpsProxy: '',
    noProxy: '',
    proxyConfigured: false,
    proxySupported: true,
    readable: true,
    invalid: false,
    note: '',
    ...overrides,
  };
}

function mountSettings() {
  const root = document.createElement('div');
  document.body.appendChild(root);
  return mount(RegistrySettings, {
    props: { server, open: true },
    attachTo: document.body,
    global: {
      // 面板只在拿到 Docker 应用根节点后才 Teleport 渲染内容。
      provide: { 'lg:docker-root': ref(root) },
      stubs: { teleport: true },
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('registry settings', () => {
  it('keeps the typed mirrors and re-reads the config when daemon.json turned invalid', async () => {
    mocks.registryConfig.mockResolvedValueOnce(config()).mockResolvedValueOnce(config());
    mocks.systemPreview.mockRejectedValueOnce({
      code: 'DAEMON_INVALID',
      message: 'daemon.json 解析失败',
      hint: '',
    });

    const wrapper = mountSettings();
    await flushPromises();

    const textarea = wrapper.get('textarea[aria-label="镜像加速源"]');
    expect((textarea.element as HTMLTextAreaElement).value).toBe(
      'https://docker.mirrors.ustc.edu.cn',
    );

    const typed = 'https://docker.mirrors.ustc.edu.cn\nhttps://mirror.example.com';
    await textarea.setValue(typed);
    await wrapper.get('button.is-primary').trigger('click');
    await flushPromises();

    // DAEMON_INVALID 的固定回归路径：多读一次配置，但用户输入不能被覆盖。
    expect(mocks.systemPreview).toHaveBeenCalledTimes(1);
    expect(mocks.registryConfig).toHaveBeenCalledTimes(2);
    expect(mocks.registryConfig).toHaveBeenLastCalledWith('srv_1');
    const after = wrapper.get('textarea[aria-label="镜像加速源"]');
    expect((after.element as HTMLTextAreaElement).value).toBe(typed);
    expect(wrapper.emitted('close')).toBeUndefined();
    wrapper.unmount();
  });

  it('locks save and close while the preview request is in flight', async () => {
    let release: (value: unknown) => void = () => {};
    mocks.registryConfig.mockResolvedValueOnce(config());
    // 预览请求挂起：模拟需要 sudo 读取 daemon.json 时的数秒等待。
    mocks.systemPreview.mockReturnValue(
      new Promise((resolve) => {
        release = resolve;
      }),
    );

    const wrapper = mountSettings();
    await flushPromises();

    await wrapper.get('button.is-primary').trigger('click');
    await flushPromises();
    // 重新查询：状态变化后 Vue 可能替换按钮节点，旧 wrapper 会读到陈旧文本。
    const save = wrapper.get('button.is-primary');
    expect(mocks.systemPreview).toHaveBeenCalledTimes(1);
    expect(save.text()).toContain('正在生成预览');
    expect(save.attributes('disabled')).toBeDefined();

    // 请求在飞时连点保存、点取消都不应产生第二个预览，也不应关闭面板。
    await save.trigger('click');
    await wrapper.get('.lgdocker-modal__foot button').trigger('click');
    await flushPromises();
    expect(mocks.systemPreview).toHaveBeenCalledTimes(1);
    expect(wrapper.emitted('close')).toBeUndefined();

    release({
      previewId: 'p1',
      kind: 'daemon.update',
      operation: '更新 daemon.json',
      target: '/etc/docker/daemon.json',
      impact: '将重启 Docker',
      rollback: '失败自动恢复',
      irreversible: true,
      expiresAtMs: Date.now() + 300_000,
      details: [],
    });
    await flushPromises();
    // 请求结束后按钮解锁，预览弹窗接管后续确认。
    expect(wrapper.get('button.is-primary').text()).toBe('保存并应用');
    wrapper.unmount();
  });
});
