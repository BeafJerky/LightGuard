import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ sftpSearch: vi.fn() }));

vi.mock('../../ssh', () => ({
  ssh: { sftpSearch: mocks.sftpSearch },
  toSshError: (error: unknown) => error,
}));

import SearchPanel from './SearchPanel.vue';

describe('SearchPanel', () => {
  it('shows results and emits the clicked entry', async () => {
    mocks.sftpSearch.mockResolvedValueOnce({
      truncated: true,
      entries: [
        { name: 'app.log', path: '/var/log/app.log', kind: 'file', size: 12, mode: 0o644, modeText: '-rw-r--r--' },
      ],
    });
    const wrapper = mount(SearchPanel, {
      props: { open: true, serverId: 'srv_1', serverName: 'web-01', root: '/var' },
      attachTo: document.body,
      global: { stubs: { teleport: true } },
    });

    await wrapper.get('input[aria-label="搜索文件"]').setValue('app');
    await wrapper.get('[data-action="search"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(mocks.sftpSearch).toHaveBeenCalledWith({
      serverId: 'srv_1',
      root: '/var',
      query: 'app',
    });
    expect(wrapper.text()).toContain('app.log');
    expect(wrapper.text()).toContain('已达结果上限');

    await wrapper.get('.search-panel__results button').trigger('click');
    expect(wrapper.emitted('open')?.[0]?.[0]).toMatchObject({ path: '/var/log/app.log' });
    wrapper.unmount();
  });
});