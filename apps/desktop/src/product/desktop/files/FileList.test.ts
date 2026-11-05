// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { nextTick } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';
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

function props(overrides: Record<string, unknown> = {}) {
  return {
    entries: [entry('a.txt')],
    selected: '',
    loading: false,
    error: '',
    truncated: false,
    sortKey: 'name' as const,
    sortDir: 'asc' as const,
    mode: 'details' as const,
    offline: false,
    resetKey: '',
    scheme: 'line' as const,
    ...overrides,
  };
}

describe('FileList', () => {
  it('emits sort when clicking a header', async () => {
    const wrapper = mount(FileList, { props: props() });
    await wrapper.find('[data-sort="size"]').trigger('click');
    expect(wrapper.emitted('sort')?.[0]).toEqual(['size']);
  });

  it('emits open on double click and select on click', async () => {
    const wrapper = mount(FileList, { props: props() });
    const row = wrapper.find('[data-path="/tmp/a.txt"]');
    await row.trigger('click');
    await row.trigger('dblclick');
    expect(wrapper.emitted('select')?.[0]?.[0]).toMatchObject({ name: 'a.txt' });
    expect(wrapper.emitted('open')?.[0]?.[0]).toMatchObject({ name: 'a.txt' });
  });

  it('shows empty, error and truncation states', async () => {
    const empty = mount(FileList, { props: props({ entries: [] }) });
    expect(empty.text()).toContain('此文件夹为空');
    const failed = mount(FileList, {
      props: props({ entries: [], error: 'FILE_NOT_FOUND 路径不存在' }),
    });
    expect(failed.text()).toContain('无法读取此目录');
    const truncated = mount(FileList, {
      props: props({ truncated: true }),
    });
    expect(truncated.text()).toContain('仅显示前 20,000 项');
  });

  it('keeps one focused viewport and resets scroll when resetKey changes', async () => {
    const wrapper = mount(FileList, {
      attachTo: document.body,
      props: props({ entries: [entry('a.txt'), entry('b.txt')], resetKey: 'a' }),
    });
    const viewport = wrapper.find('.lgfiles-list__viewport').element as HTMLElement;
    viewport.scrollTop = 90;
    viewport.dataset.marker = 'same-instance';
    const outside = document.createElement('input');
    document.body.append(outside);
    outside.focus();
    expect(document.activeElement).toBe(outside);

    await wrapper.setProps({ resetKey: 'b' });
    await nextTick();
    await flushPromises();

    expect(viewport.scrollTop).toBe(0);
    expect(document.activeElement).toBe(viewport);
    // The component instance must survive navigation, not remount.
    expect(wrapper.find('.lgfiles-list__viewport').element).toBe(viewport);
    expect(viewport.dataset.marker).toBe('same-instance');

    outside.remove();
    wrapper.unmount();
  });

  it('handles arrows, Enter, Backspace and F5 on the viewport', async () => {
    const wrapper = mount(FileList, {
      props: props({
        entries: [entry('a.txt'), entry('b.txt')],
        selected: '/tmp/a.txt',
      }),
    });
    const viewport = wrapper.find('.lgfiles-list__viewport');

    await viewport.trigger('keydown', { key: 'ArrowDown' });
    expect(wrapper.emitted('select')?.[0]?.[0]).toMatchObject({ name: 'b.txt' });

    await viewport.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('open')?.[0]?.[0]).toMatchObject({ name: 'a.txt' });

    await viewport.trigger('keydown', { key: 'Backspace' });
    expect(wrapper.emitted('back')).toHaveLength(1);

    await viewport.trigger('keydown', { key: 'F5' });
    expect(wrapper.emitted('refresh')).toHaveLength(1);
  });

  it('renders a compact list without the details header', async () => {
    const wrapper = mount(FileList, {
      props: props({ mode: 'list', entries: [entry('a.txt'), entry('b.txt')] }),
    });

    expect(wrapper.find('.lgfiles-list__head').exists()).toBe(false);
    expect(wrapper.findAll('.lgfiles-list__row.is-compact')).toHaveLength(2);
    expect(wrapper.find('.lgfiles-list__cell.is-time').exists()).toBe(false);
    expect(wrapper.find('[data-path="/tmp/b.txt"]').text()).toContain('b.txt');
  });

  it('renders an icon grid with selectable cells', async () => {
    const wrapper = mount(FileList, {
      props: props({ mode: 'grid', entries: [entry('a.txt'), entry('b.txt')] }),
    });

    expect(wrapper.find('.lgfiles-list__head').exists()).toBe(false);
    expect(wrapper.findAll('.lgfiles-grid__cell')).toHaveLength(2);
    const cell = wrapper.find('[data-path="/tmp/b.txt"]');
    await cell.trigger('click');
    await cell.trigger('dblclick');
    expect(wrapper.emitted('select')?.[0]?.[0]).toMatchObject({ name: 'b.txt' });
    expect(wrapper.emitted('open')?.[0]?.[0]).toMatchObject({ name: 'b.txt' });
  });

  it('renders only the visible window for a 20k listing', () => {
    const entries = Array.from({ length: 20_000 }, (_, index) => entry(`file${index}.txt`));
    const wrapper = mount(FileList, { props: props({ entries }) });
    expect(wrapper.findAll('[data-path]').length).toBeLessThan(50);
    expect(wrapper.text()).toContain('file0.txt');
  });

  it('renders line icons with category tones', () => {
    const wrapper = mount(FileList, {
      props: props({ entries: [entry('main.rs'), entry('notes', 'dir')] }),
    });
    const code = wrapper.find('[data-path="/tmp/main.rs"] .lgfiles-file-icon');
    expect(code.find('use').attributes('href')).toBe('#lg-i-file-code');
    expect(code.classes()).toContain('is-tone-info');
    const folder = wrapper.find('[data-path="/tmp/notes"] .lgfiles-file-icon');
    expect(folder.find('use').attributes('href')).toBe('#lg-i-folder');
    expect(folder.classes()).toContain('is-folder');
  });

  it('renders the color sprite and falls back to line icons for links', () => {
    const wrapper = mount(FileList, {
      props: props({
        scheme: 'color',
        entries: [entry('main.rs'), entry('lost', 'symlink')],
      }),
    });
    expect(wrapper.find('symbol#lg-fv-rust').exists()).toBe(true);
    expect(wrapper.find('[data-path="/tmp/main.rs"] use').attributes('href')).toBe('#lg-fv-rust');
    expect(wrapper.find('[data-path="/tmp/lost"] use').attributes('href')).toBe('#lg-i-link');
  });
});
