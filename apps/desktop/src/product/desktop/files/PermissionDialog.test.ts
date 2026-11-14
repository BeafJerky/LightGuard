import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

vi.mock('../../guard/store', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../guard/store')>();
  return {
    ...actual,
    previewWrite: vi.fn(),
    executePreview: vi.fn(),
    discardPreview: vi.fn(),
  };
});

import { previewWrite } from '../../guard/store';
import PermissionDialog from './PermissionDialog.vue';

describe('PermissionDialog', () => {
  it('uses presets and previews the chosen mode', async () => {
    const wrapper = mount(PermissionDialog, {
      props: {
        open: true,
        serverId: 'srv_1',
        serverName: 'web-01',
        path: '/data/x',
        currentMode: '644',
      },
      attachTo: document.body,
      global: { stubs: { teleport: true } },
    });

    await wrapper.get('[data-preset="755"]').trigger('click');
    await wrapper.get('[data-action="preview"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(previewWrite).toHaveBeenCalledWith('srv_1', 'chmod', {
      path: '/data/x',
      mode: '755',
    });
    wrapper.unmount();
  });
});