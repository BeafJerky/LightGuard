// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { ref } from 'vue';
import type { Server } from '../types';

const mocks = vi.hoisted(() => ({
  registryTags: vi.fn(),
}));

// 页面只依赖只读的版本查询；写操作在其它用例里覆盖。
vi.mock('./api', () => ({
  dockerApi: { available: true, registryTags: mocks.registryTags },
}));

vi.mock('../ssh', () => ({
  ssh: {},
  toSshError: (error: unknown) => error,
}));

import RegistryPage from './RegistryPage.vue';

const server = { id: 'srv_1' } as Server;

function mountPage() {
  return mount(RegistryPage, {
    props: { server },
    attachTo: document.body,
    global: {
      // 弹窗 Teleport 到 Docker 应用根节点；测试里还原成内联渲染。
      stubs: { teleport: true },
      provide: { 'lg:docker-root': ref(document.body) },
    },
  });
}

type Page = ReturnType<typeof mountPage>;

async function openPull(page: Page): Promise<void> {
  await page.find('.lgdocker-quick__item button').trigger('click');
  await flushPromises();
}

function tagInput(page: Page) {
  return page.find<HTMLInputElement>('input[aria-label="镜像标签"]');
}

afterEach(() => {
  mocks.registryTags.mockReset();
  document.body.innerHTML = '';
});

describe('pull dialog tag combobox', () => {
  it('loads versions when the dialog opens and lists them newest first', async () => {
    mocks.registryTags.mockResolvedValue({
      repo: 'library/nginx',
      tags: ['1.24', 'latest', '1.25-alpine', '1.25'],
    });
    const page = mountPage();
    await openPull(page);
    expect(mocks.registryTags).toHaveBeenCalledWith('srv_1', 'nginx');

    await page.find('button[aria-label="选择镜像版本"]').trigger('click');
    const options = page.findAll('[role="option"]');
    expect(options.map((option) => option.text())).toEqual([
      'latest',
      '1.25',
      '1.25-alpine',
      '1.24',
    ]);
    page.unmount();
  });

  it('fills the tag and reference when a version is picked', async () => {
    mocks.registryTags.mockResolvedValue({
      repo: 'library/nginx',
      tags: ['1.24', 'latest', '1.25-alpine'],
    });
    const page = mountPage();
    await openPull(page);
    await page.find('button[aria-label="选择镜像版本"]').trigger('click');
    const option = page.findAll('[role="option"]').find((item) => item.text() === '1.25-alpine');
    expect(option).toBeTruthy();
    await option!.trigger('click');

    expect(tagInput(page).element.value).toBe('1.25-alpine');
    expect(page.text()).toContain('nginx:1.25-alpine');
    expect(page.find('[role="listbox"]').exists()).toBe(false);
    page.unmount();
  });

  it('filters the list as the tag is typed but keeps free input', async () => {
    mocks.registryTags.mockResolvedValue({
      repo: 'library/nginx',
      tags: ['1.24', 'latest', '1.25-alpine', '1.25'],
    });
    const page = mountPage();
    await openPull(page);
    await page.find('button[aria-label="选择镜像版本"]').trigger('click');
    await tagInput(page).setValue('alp');

    expect(page.findAll('[role="option"]').map((option) => option.text())).toEqual([
      '1.25-alpine',
    ]);
    expect(page.text()).toContain('nginx:alp');
    page.unmount();
  });

  it('selects a version with the keyboard', async () => {
    mocks.registryTags.mockResolvedValue({
      repo: 'library/nginx',
      tags: ['1.24', 'latest', '1.25-alpine', '1.25'],
    });
    const page = mountPage();
    await openPull(page);
    // Teleport stub 会在每次更新时重建节点，这里每次重新取输入框（真实窗口不会）。
    await tagInput(page).trigger('keydown', { key: 'ArrowDown' });
    await tagInput(page).trigger('keydown', { key: 'ArrowDown' });
    await tagInput(page).trigger('keydown', { key: 'Enter' });

    expect(tagInput(page).element.value).toBe('1.25');
    expect(page.find('[role="listbox"]').exists()).toBe(false);
    page.unmount();
  });

  it('retries loading versions from the dropdown', async () => {
    mocks.registryTags
      .mockRejectedValueOnce(new Error('连接 Docker Hub 超时'))
      .mockResolvedValueOnce({ repo: 'library/nginx', tags: ['1.24', 'latest'] });
    const page = mountPage();
    await openPull(page);
    await page.find('button[aria-label="选择镜像版本"]').trigger('click');
    await flushPromises();
    expect(page.find('[role="listbox"]').text()).toContain('连接 Docker Hub 超时');

    await page.find('[role="listbox"] button').trigger('click');
    await flushPromises();
    expect(page.findAll('[role="option"]').map((option) => option.text())).toEqual([
      'latest',
      '1.24',
    ]);
    page.unmount();
  });

  it('keeps the dialog usable when versions cannot be loaded', async () => {
    mocks.registryTags.mockRejectedValue(new Error('连接 Docker Hub 超时'));
    const page = mountPage();
    await openPull(page);
    await page.find('button[aria-label="选择镜像版本"]').trigger('click');
    await flushPromises();

    expect(page.find('[role="listbox"]').text()).toContain('连接 Docker Hub 超时');
    expect(page.find('[role="listbox"]').text()).toContain('重试');

    await tagInput(page).setValue('1.19');
    expect(page.text()).toContain('nginx:1.19');
    page.unmount();
  });
});
