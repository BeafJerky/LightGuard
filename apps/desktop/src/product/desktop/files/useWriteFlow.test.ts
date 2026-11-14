import { beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

vi.mock('../../guard/store', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../guard/store')>();
  return {
    ...actual,
    previewWrite: vi.fn(),
    executePreview: vi.fn(),
    discardPreview: vi.fn(),
  };
});

import { guardState, previewWrite } from '../../guard/store';
import { useWriteFlow } from './useWriteFlow';

describe('useWriteFlow', () => {
  beforeEach(() => {
    guardState.preview = null;
    guardState.previewServerId = '';
    guardState.error = '';
    guardState.errorHint = '';
    guardState.errorCode = '';
    vi.clearAllMocks();
  });

  it('only accepts a preview for the same server, kind and target', async () => {
    const flow = useWriteFlow(() => 'srv_1', 'rename', () => '/data/new');
    guardState.preview = {
      previewId: 'p1',
      kind: 'rename',
      path: '/data/old',
      operation: '重命名',
      target: '/data/new',
      impact: '',
      rollback: '',
      irreversible: false,
      expiresAtMs: Date.now() + 60_000,
      details: {},
    };
    guardState.previewServerId = 'srv_1';
    await nextTick();
    expect(flow.preview.value?.previewId).toBe('p1');

    guardState.previewServerId = 'srv_2';
    await nextTick();
    expect(flow.preview.value).toBeNull();
  });

  it('reports preview failures from the guard store', async () => {
    vi.mocked(previewWrite).mockImplementationOnce(async () => {
      guardState.error = '目标已存在';
      guardState.errorHint = '请换一个名称';
      guardState.errorCode = 'PREVIEW_FAILED';
      return null;
    });
    const flow = useWriteFlow(() => 'srv_1', 'rename', () => '/data/new');
    await flow.runPreview({ path: '/data/old', newName: 'new' });

    expect(flow.failure.value?.kind).toBe('preview');
    expect(flow.failure.value?.message).toContain('目标已存在');
    expect(vi.mocked(previewWrite)).toHaveBeenCalledWith('srv_1', 'rename', {
      path: '/data/old',
      newName: 'new',
    });
  });
});