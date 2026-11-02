import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { relaunch } from '@tauri-apps/plugin-process';
import UpdatePanel from './UpdatePanel.vue';
import { updateController } from './index';
import { dismissToast, toasts } from '../ui';

// 模拟惰性分块加载失败：面板必须捕获它，而不是留下未处理拒绝。
vi.mock('@tauri-apps/plugin-opener', () => {
  throw new Error('failed to load opener chunk');
});

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(async () => {}),
}));

beforeEach(() => {
  vi.restoreAllMocks();
  updateController.state.phase = 'idle';
  updateController.state.info = null;
  updateController.state.error = null;
  updateController.state.progress = { downloaded: 0, total: null };
  updateController.state.failedAction = null;
  toasts.slice().forEach((item) => dismissToast(item.id));
  vi.mocked(relaunch).mockResolvedValue(undefined);
  vi.clearAllMocks();
});

const readyInfo = {
  version: '0.14.1',
  currentVersion: '0.14.0',
  date: '2026-11-02T00:00:00Z',
  body: '修复与打磨',
  size: 6 * 1024 * 1024,
};

async function clickButton(wrapper: VueWrapper, label: string) {
  const button = wrapper.findAll('button').find((item) => item.text() === label);
  if (!button) throw new Error(`button not found: ${label}`);
  await button.trigger('click');
  await flushPromises();
}

describe('UpdatePanel', () => {
  it('shows the available version and size', async () => {
    updateController.state.phase = 'available';
    updateController.state.info = {
      version: '0.14.0',
      currentVersion: '0.13.0',
      date: '2026-11-02T00:00:00Z',
      body: '修复与打磨',
      size: 12 * 1024 * 1024,
    };
    const wrapper = mount(UpdatePanel);
    await flushPromises();
    expect(wrapper.text()).toContain('v0.14.0');
    expect(wrapper.text()).toContain('12.0 MB');
    expect(wrapper.text()).toContain('下载并安装');
  });

  it('shows a classified error with the release link', async () => {
    updateController.state.phase = 'error';
    updateController.state.error = { kind: 'signature', message: '更新包签名校验失败，已拒绝安装。' };
    const wrapper = mount(UpdatePanel);
    expect(wrapper.text()).toContain('签名校验失败');
    expect(wrapper.text()).toContain('打开发布页');
  });

  it('disables the check button once the package is ready', async () => {
    updateController.state.phase = 'ready';
    updateController.state.info = readyInfo;
    const wrapper = mount(UpdatePanel);
    const check = wrapper.findAll('button').find((item) => item.text() === '已就绪');
    expect(check?.attributes('disabled')).toBeDefined();
    expect(wrapper.text()).toContain('更新包已就绪，重启后安装。');
  });

  it('retries the install step after an install failure', async () => {
    updateController.state.phase = 'error';
    updateController.state.info = readyInfo;
    updateController.state.error = { kind: 'unknown', message: '更新失败，请稍后重试。' };
    updateController.state.failedAction = 'install';
    const install = vi.spyOn(updateController, 'install').mockResolvedValue(false);
    const download = vi.spyOn(updateController, 'download').mockResolvedValue();
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '重试');
    expect(install).toHaveBeenCalledOnce();
    expect(download).not.toHaveBeenCalled();
  });

  it('retries the download step after a download failure', async () => {
    updateController.state.phase = 'error';
    updateController.state.info = readyInfo;
    updateController.state.error = { kind: 'network', message: '无法访问更新服务器。' };
    updateController.state.failedAction = 'download';
    const install = vi.spyOn(updateController, 'install').mockResolvedValue(false);
    const download = vi.spyOn(updateController, 'download').mockResolvedValue();
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '重试');
    expect(download).toHaveBeenCalledOnce();
    expect(install).not.toHaveBeenCalled();
  });

  it('retries the check step after a failed manual check, even with stale info', async () => {
    updateController.state.phase = 'error';
    updateController.state.info = readyInfo;
    updateController.state.error = { kind: 'network', message: '无法访问更新服务器。' };
    updateController.state.failedAction = 'check';
    const check = vi.spyOn(updateController, 'check').mockResolvedValue();
    const download = vi.spyOn(updateController, 'download').mockResolvedValue();
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '重试');
    expect(check).toHaveBeenCalledWith({ manual: true });
    expect(download).not.toHaveBeenCalled();
  });

  it('does not relaunch when the installer fails', async () => {
    updateController.state.phase = 'ready';
    updateController.state.info = readyInfo;
    vi.spyOn(updateController, 'install').mockResolvedValue(false);
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '重启并安装');
    expect(relaunch).not.toHaveBeenCalled();
  });

  it('surfaces a relaunch failure and retries only the relaunch', async () => {
    updateController.state.phase = 'ready';
    updateController.state.info = readyInfo;
    const install = vi.spyOn(updateController, 'install').mockResolvedValue(true);
    vi.mocked(relaunch).mockRejectedValueOnce(new Error('relaunch failed'));
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '重启并安装');
    expect(updateController.state.phase).toBe('error');
    expect(updateController.state.failedAction).toBe('relaunch');
    expect(updateController.state.error?.message).toContain('重新打开 LightGuard');

    await clickButton(wrapper, '重试');
    expect(relaunch).toHaveBeenCalledTimes(2);
    expect(install).toHaveBeenCalledOnce();
  });

  it('cancels a running download', async () => {
    const cancel = vi.fn(async () => {});
    updateController.state.phase = 'downloading';
    updateController.state.info = { version: '0.15.0', currentVersion: '0.14.1' };
    updateController.state.progress = { downloaded: 10, total: 100 };
    vi.spyOn(updateController, 'cancel').mockImplementation(cancel);
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '取消');
    expect(cancel).toHaveBeenCalledTimes(1);
  });

  it('defers a ready package', async () => {
    const defer = vi.fn(async () => {});
    updateController.state.phase = 'ready';
    updateController.state.info = { version: '0.15.0', currentVersion: '0.14.1' };
    vi.spyOn(updateController, 'defer').mockImplementation(defer);
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '稍后');
    expect(defer).toHaveBeenCalledTimes(1);
    expect(toasts.at(-1)?.title).toBe('已保留更新包');
  });

  it('warns when the defer command fails and stays ready', async () => {
    updateController.state.phase = 'ready';
    updateController.state.info = { version: '0.15.0', currentVersion: '0.14.1' };
    vi.spyOn(updateController, 'defer').mockRejectedValue(new Error('defer failed'));
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '稍后');
    expect(toasts.at(-1)?.title).toBe('无法记录稍后安装');
    expect(toasts.at(-1)?.tone).toBe('warning');
    expect(updateController.state.phase).toBe('ready');
  });

  it('hides the defer button when the package is already deferred across restarts', async () => {
    updateController.state.phase = 'ready';
    updateController.state.info = { ...readyInfo, cached: true, deferred: true };
    const wrapper = mount(UpdatePanel);
    expect(wrapper.text()).not.toContain('稍后');
    expect(wrapper.text()).toContain('重启并安装');
  });

  it('explains the mirror fallback on a network error', () => {
    updateController.state.phase = 'error';
    updateController.state.error = { kind: 'network', message: '无法访问更新服务器。' };
    const wrapper = mount(UpdatePanel);
    expect(wrapper.text()).toContain('已尝试直连与镜像，可在下方手动下载。');
  });

  it('reports a failed external link instead of an unhandled rejection', async () => {
    updateController.state.phase = 'error';
    updateController.state.error = { kind: 'network', message: '无法访问更新服务器。' };
    const wrapper = mount(UpdatePanel);
    await clickButton(wrapper, '打开发布页');
    expect(toasts.some((item) => item.title === '无法打开发布页')).toBe(true);
  });
});
