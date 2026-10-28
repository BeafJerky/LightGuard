// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import FilesApp from './FilesApp.vue';
import type { Server } from '../../types';

function server(overrides: Partial<Server> = {}): Server {
  return {
    id: 'srv_1',
    sessionId: '',
    status: 'offline',
    disks: [],
    ...overrides,
  } as Server;
}

describe('FilesApp', () => {
  it('asks to connect when there is no session', () => {
    const wrapper = mount(FilesApp, { props: { server: server() } });
    expect(wrapper.text()).toContain('请先连接服务器');
    expect(wrapper.find('.lgfiles-toolbar').exists()).toBe(false);
  });

  it('shows offline overlay when the session dropped', () => {
    const wrapper = mount(FilesApp, {
      props: { server: server({ sessionId: 's1', status: 'offline' }) },
    });
    expect(wrapper.text()).toContain('连接已断开');
  });

  it('removes its window listeners when unmounted', () => {
    const add = vi.spyOn(window, 'addEventListener');
    const remove = vi.spyOn(window, 'removeEventListener');
    const wrapper = mount(FilesApp, { props: { server: server() } });
    wrapper.unmount();

    const added = add.mock.calls.map(([type]) => type);
    const removed = remove.mock.calls.map(([type]) => type);
    expect(added).toEqual(expect.arrayContaining(['keydown', 'pointerdown']));
    expect(removed).toEqual(expect.arrayContaining(['keydown', 'pointerdown']));

    add.mockRestore();
    remove.mockRestore();
  });
});
