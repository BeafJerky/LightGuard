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
import DeleteDialog from './DeleteDialog.vue';

describe('DeleteDialog', () => {
  it('requires the irreversible acknowledgement before executing', async () => {
    vi.mocked(previewWrite).mockImplementationOnce(async () => {
      guardState.preview = {
        previewId: 'p1',
        kind: 'delete',
        path: '/data/tmp',
        operation: '删除',
        target: '/data/tmp',
        impact: '将递归删除 /data/tmp，12 项',
        rollback: '删除不可回退',
        irreversible: true,
        expiresAtMs: Date.now() + 60_000,
        details: { entries: 12, bytes: 34, truncated: false },
      };
      guardState.previewServerId = 'srv_1';
      return guardState.preview;
    });
    vi.mocked(executePreview).mockResolvedValueOnce({ auditId: 4, path: '/data/tmp' });

    const wrapper = mount(DeleteDialog, {
      props: {
        open: true,
        serverId: 'srv_1',
        serverName: 'web-01',
        path: '/data/tmp',
        kind: 'dir',
      },
      attachTo: document.body,
      global: { stubs: { teleport: true } },
    });

    await wrapper.get('[data-action="preview"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(wrapper.text()).toContain('12');
    expect(wrapper.get('[data-action="confirm"]').attributes('disabled')).toBeDefined();

    await wrapper.get('input[aria-label="我了解删除后无法恢复"]').setValue(true);
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(executePreview).toHaveBeenCalled();
    expect(wrapper.emitted('deleted')).toBeTruthy();
    wrapper.unmount();
  });
});