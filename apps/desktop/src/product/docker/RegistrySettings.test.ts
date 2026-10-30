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

function mountSettings(mode: 'mirrors' | 'proxy' = 'mirrors') {
  const root = document.createElement('div');
  document.body.appendChild(root);
  return mount(RegistrySettings, {
    props: { server, open: true, mode },
    attachTo: document.body,
    global: {
      // 面板只在拿到 Docker 应用根节点后才 Teleport 渲染内容。
      provide: { 'lg:docker-root': ref(root) },
      stubs: { teleport: true },
    },
  });
}

function previewResult() {
  return {
    previewId: 'p1',
    kind: 'daemon.update',
    operation: '更新 daemon.json',
    target: '/etc/docker/daemon.json',
    impact: '将重启 Docker',
    rollback: '失败自动恢复',
    irreversible: true,
    expiresAtMs: Date.now() + 300_000,
    details: [],
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  window.localStorage.clear();
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('registry settings', () => {
  it('rejects a non-http mirror before asking for a preview', async () => {
    mocks.registryConfig.mockResolvedValueOnce(config());
    const wrapper = mountSettings();
    await flushPromises();

    await wrapper.get('input[aria-label="加速源地址"]').setValue('ftp://mirror.example.com');
    await wrapper.get('.lgdocker-modal__foot .is-primary').trigger('click');
    await flushPromises();

    expect(mocks.systemPreview).not.toHaveBeenCalled();
    expect(wrapper.get('.lgdocker-field-error').text()).toContain('必须是 http:// 或 https:// URL');
    wrapper.unmount();
  });

  it('keeps the typed mirrors and re-reads the config when daemon.json turned invalid', async () => {
    mocks.registryConfig.mockResolvedValueOnce(config()).mockResolvedValueOnce(config());
    mocks.systemPreview.mockRejectedValueOnce({
      code: 'DAEMON_INVALID',
      message: 'daemon.json 解析失败',
      hint: '',
    });

    const wrapper = mountSettings();
    await flushPromises();
    expect(wrapper.get('input[aria-label="加速源地址"]').attributes('value')).toBeUndefined();
    expect((wrapper.get('input[aria-label="加速源地址"]').element as HTMLInputElement).value).toBe(
      'https://docker.mirrors.ustc.edu.cn',
    );

    const typed = 'https://mirror.example.com';
    await wrapper.get('input[aria-label="加速源地址"]').setValue(typed);
    await wrapper.get('.lgdocker-modal__foot .is-primary').trigger('click');
    await flushPromises();

    // DAEMON_INVALID 的固定回归路径：多读一次配置，但用户输入不能被覆盖。
    expect(mocks.systemPreview).toHaveBeenCalledTimes(1);
    expect(mocks.registryConfig).toHaveBeenCalledTimes(2);
    expect(mocks.registryConfig).toHaveBeenLastCalledWith('srv_1');
    const after = wrapper.get('input[aria-label="加速源地址"]');
    expect((after.element as HTMLInputElement).value).toBe(typed);
    expect(wrapper.emitted('close')).toBeUndefined();
    wrapper.unmount();
  });

  it('adds a mirror row and sends the ordered list', async () => {
    mocks.registryConfig.mockResolvedValueOnce(config());
    mocks.systemPreview.mockResolvedValueOnce(previewResult());
    const wrapper = mountSettings();
    await flushPromises();

    await wrapper.get('.lgdocker-mirror__bar button').trigger('click');
    await flushPromises();
    const inputs = wrapper.findAll('input[aria-label="加速源地址"]');
    expect(inputs).toHaveLength(2);
    await inputs[1].setValue('https://mirror.example.com');
    await wrapper.get('.lgdocker-modal__foot .is-primary').trigger('click');
    await flushPromises();

    expect(mocks.systemPreview).toHaveBeenCalledWith(
      'srv_1',
      {
        kind: 'daemon.update',
        payload: {
          mirrors: ['https://docker.mirrors.ustc.edu.cn', 'https://mirror.example.com'],
        },
      },
      undefined,
    );
    wrapper.unmount();
  });

  it('encodes proxy credentials into the daemon payload', async () => {
    mocks.registryConfig.mockResolvedValueOnce(config());
    mocks.systemPreview.mockResolvedValueOnce(previewResult());
    const wrapper = mountSettings('proxy');
    await flushPromises();

    await wrapper.get('input[aria-label="开启代理"]').setValue(true);
    await wrapper.get('input[aria-label="代理服务器"]').setValue('http://proxy:3128');
    await wrapper.get('input[aria-label="代理账号"]').setValue('user');
    await wrapper.get('input[aria-label="代理密码"]').setValue('pa:ss');
    await wrapper.get('textarea[aria-label="不代理域名"]').setValue('localhost\n10.0.0.0/8');
    await wrapper.get('.lgdocker-modal__foot .is-primary').trigger('click');
    await flushPromises();

    expect(mocks.systemPreview).toHaveBeenCalledWith(
      'srv_1',
      {
        kind: 'daemon.update',
        payload: {
          httpProxy: 'http://user:pa%3Ass@proxy:3128',
          httpsProxy: 'http://user:pa%3Ass@proxy:3128',
          noProxy: 'localhost,10.0.0.0/8',
        },
      },
      undefined,
    );
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

    await wrapper.get('.lgdocker-modal__foot .is-primary').trigger('click');
    await flushPromises();
    // 重新查询：状态变化后 Vue 可能替换按钮节点，旧 wrapper 会读到陈旧文本。
    const save = wrapper.get('.lgdocker-modal__foot .is-primary');
    expect(mocks.systemPreview).toHaveBeenCalledTimes(1);
    expect(save.text()).toContain('正在生成预览');
    expect(save.attributes('disabled')).toBeDefined();

    // 请求在飞时连点保存、点取消都不应产生第二个预览，也不应关闭面板。
    await save.trigger('click');
    await wrapper.get('.lgdocker-modal__foot button').trigger('click');
    await flushPromises();
    expect(mocks.systemPreview).toHaveBeenCalledTimes(1);
    expect(wrapper.emitted('close')).toBeUndefined();

    release(previewResult());
    await flushPromises();
    // 请求结束后按钮解锁，预览弹窗接管后续确认。
    expect(wrapper.get('.lgdocker-modal__foot .is-primary').text()).toBe('保存并重启 Docker');
    wrapper.unmount();
  });
});
