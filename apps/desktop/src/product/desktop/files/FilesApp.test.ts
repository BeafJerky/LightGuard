// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
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
});
