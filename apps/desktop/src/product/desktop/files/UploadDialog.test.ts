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
import UploadDialog from './UploadDialog.vue';

describe('UploadDialog', () => {
  it('previews conflict policy and executes with progress channel', async () => {
    vi.mocked(previewWrite).mockImplementationOnce(async () => {
      guardState.preview = {
        previewId: 'up1',
        kind: 'upload',
        path: '/data',
        operation: '上传文件',
        target: '/data',
        impact: '将上传 1 个文件',
        rollback: 'create 可回退',
        irreversible: false,
        expiresAtMs: Date.now() + 60_000,
        details: {
          files: [
            { name: 'a.txt', remotePath: '/data/a.txt', size: 3, action: 'create' },
          ],
        },
      };
      guardState.previewServerId = 'srv_1';
      return guardState.preview;
    });
    vi.mocked(executePreview).mockResolvedValueOnce({ auditId: 9, path: '/data/a.txt' });

    const wrapper = mount(UploadDialog, {
      props: {
        open: true,
        serverId: 'srv_1',
        serverName: 'web-01',
        dir: '/data',
        files: [{ path: 'C:/tmp/a.txt', name: 'a.txt', size: 3 }],
      },
      attachTo: document.body,
      global: { stubs: { teleport: true } },
    });

    await wrapper.get('[data-policy="rename"]').trigger('click');
    await wrapper.get('[data-action="preview"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(previewWrite).toHaveBeenCalledWith('srv_1', 'upload', {
      dir: '/data',
      conflictPolicy: 'rename',
      files: [{ path: 'C:/tmp/a.txt', name: 'a.txt', size: 3 }],
    });
    expect(wrapper.text()).toContain('新建');

    await wrapper.get('[data-action="confirm"]').trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(executePreview).toHaveBeenCalled();
    expect(wrapper.emitted('uploaded')?.[0]?.[0]).toBe(1);
    wrapper.unmount();
  });
});