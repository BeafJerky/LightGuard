// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import FileList from './FileList.vue';
import type { FileEntry } from '../../ssh/backend';

function entry(name: string, kind: FileEntry['kind'] = 'file'): FileEntry {
  return {
    name,
    path: `/tmp/${name}`,
    kind,
    size: 1,
    mtime: 0,
    mode: 0o644,
    modeText: '-rw-r--r--',
    uid: 0,
    gid: 0,
  };
}

describe('FileList', () => {
  it('emits sort when clicking a header', async () => {
    const wrapper = mount(FileList, {
      props: { entries: [entry('a.txt')], selected: '', loading: false, error: '', truncated: false, sortKey: 'name', sortDir: 'asc', offline: false },
    });
    await wrapper.find('[data-sort="size"]').trigger('click');
    expect(wrapper.emitted('sort')?.[0]).toEqual(['size']);
  });

  it('emits open on double click and select on click', async () => {
    const wrapper = mount(FileList, {
      props: { entries: [entry('a.txt')], selected: '', loading: false, error: '', truncated: false, sortKey: 'name', sortDir: 'asc', offline: false },
    });
    const row = wrapper.find('[data-path="/tmp/a.txt"]');
    await row.trigger('click');
    await row.trigger('dblclick');
    expect(wrapper.emitted('select')?.[0]?.[0]).toMatchObject({ name: 'a.txt' });
    expect(wrapper.emitted('open')?.[0]?.[0]).toMatchObject({ name: 'a.txt' });
  });

  it('shows empty, error and truncation states', async () => {
    const empty = mount(FileList, {
      props: { entries: [], selected: '', loading: false, error: '', truncated: false, sortKey: 'name', sortDir: 'asc', offline: false },
    });
    expect(empty.text()).toContain('此文件夹为空');
    const failed = mount(FileList, {
      props: { entries: [], selected: '', loading: false, error: 'FILE_NOT_FOUND 路径不存在', truncated: false, sortKey: 'name', sortDir: 'asc', offline: false },
    });
    expect(failed.text()).toContain('无法读取此目录');
    const truncated = mount(FileList, {
      props: { entries: [entry('a.txt')], selected: '', loading: false, error: '', truncated: true, sortKey: 'name', sortDir: 'asc', offline: false },
    });
    expect(truncated.text()).toContain('仅显示前 20,000 项');
  });
});
