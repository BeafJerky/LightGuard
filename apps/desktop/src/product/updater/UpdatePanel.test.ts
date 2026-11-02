import { beforeEach, describe, expect, it } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import UpdatePanel from './UpdatePanel.vue';
import { updateController } from './index';

beforeEach(() => {
  updateController.state.phase = 'idle';
  updateController.state.info = null;
  updateController.state.error = null;
  updateController.state.progress = { downloaded: 0, total: null };
});

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
});
