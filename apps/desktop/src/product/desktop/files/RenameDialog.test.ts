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

import { executePreview, guardState, previewWrite } from '../../guard/store';
import RenameDialog from './RenameDialog.vue';

describe('RenameDialog', () => {
  it('previews and executes a rename', async () => {
    vi.mocked(previewWrite).mockImplementationOnce(async () => {
      guardState.preview = {
        previewId: 'p1',
        kind: 'rename',
        path: '/data/old',
        operation: '重命名',
        target: '/data/old → /data/new',
        impact: '将把 /data/old 重命名为 /data/new',
        rollback: '可回退',
        irreversible: false,
        expiresAtMs: Date.now() + 60_000,
        details: {},
      };
      guardState.previewServerId = 'srv_1';
      return guardState.preview;
    });
    vi.mocked(executePreview).mockResolvedValueOnce({ auditId: 3, path: '/data/new' });

    const wrapper = mount(RenameDialog, {
      props: {
        open: true,
        serverId: 'srv_1',
        serverName: 'web-01',
        path: '/data/old',
        currentName: 'old',
      },
      attachTo: document.body,
      global: { stubs: { teleport: true } },
    });

    await wrapper.get('input[aria-label="新名称"]').setValue('new');
    await wrapper.get('[data-action="preview"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(previewWrite).toHaveBeenCalledWith('srv_1', 'rename', {
      path: '/data/old',
      newName: 'new',
    });

    await wrapper.get('[data-action="confirm"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(executePreview).toHaveBeenCalled();
    expect(wrapper.emitted('renamed')?.[0]?.[0]).toBe('/data/new');
    wrapper.unmount();
  });
});