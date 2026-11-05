// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { nextTick } from 'vue';
import { mount } from '@vue/test-utils';
import FileTabBar from './FileTabBar.vue';

const tabs = [
  { key: 'a', title: '主目录', path: '/root' },
  { key: 'b', title: '配置', path: '/etc/nginx' },
];

describe('FileTabBar', () => {
  it('renders tabs and marks the active one', () => {
    const wrapper = mount(FileTabBar, { props: { tabs, activeKey: 'b' } });
    const items = wrapper.findAll('.lgfiles-tab');
    expect(items.map((item) => item.text())).toEqual(['主目录', '配置']);
    expect(items[0].attributes('aria-selected')).toBe('false');
    expect(items[1].attributes('aria-selected')).toBe('true');
    expect(items[0].attributes('title')).toBe('/root');
  });

  it('emits activate, close and create', async () => {
    const wrapper = mount(FileTabBar, { props: { tabs, activeKey: 'a' } });

    await wrapper.findAll('.lgfiles-tab')[1].trigger('click');
    await wrapper.findAll('.lgfiles-tab__close')[0].trigger('click');
    await wrapper.find('.lgfiles-tab-add').trigger('click');

    expect(wrapper.emitted('activate')?.[0]).toEqual(['b']);
    expect(wrapper.emitted('close')?.[0]).toEqual(['a']);
    expect(wrapper.emitted('create')).toHaveLength(1);
  });

  it('closes a tab with the middle mouse button', async () => {
    const wrapper = mount(FileTabBar, { props: { tabs, activeKey: 'a' } });
    await wrapper.findAll('.lgfiles-tab')[1].trigger('auxclick', { button: 1 });
    expect(wrapper.emitted('close')?.[0]).toEqual(['b']);
  });

  it('marks interactive parts as drag-ignored', () => {
    const wrapper = mount(FileTabBar, { props: { tabs, activeKey: 'a' } });
    expect(wrapper.findAll('[data-win-drag-ignore]')).toHaveLength(3);
  });

  it('keeps a fixed drag zone outside the scrollable tab strip', () => {
    const wrapper = mount(FileTabBar, { props: { tabs, activeKey: 'a' } });
    const strip = wrapper.find('.lgfiles-tabbar__strip');
    const drag = wrapper.find('.lgfiles-tabbar__drag');
    expect(strip.findAll('.lgfiles-tab')).toHaveLength(2);
    expect(strip.find('.lgfiles-tab-add').exists()).toBe(false);
    expect(drag.exists()).toBe(true);
    expect(drag.attributes('data-win-drag-ignore')).toBeUndefined();
  });

  it('scrolls the active tab into view when tabs overflow', async () => {
    const wrapper = mount(FileTabBar, { props: { tabs, activeKey: 'a' } });
    const strip = wrapper.find('.lgfiles-tabbar__strip').element as HTMLElement;
    strip.getBoundingClientRect = () =>
      ({ left: 0, right: 300, top: 0, bottom: 36, width: 300, height: 36 }) as DOMRect;
    const second = wrapper.findAll('.lgfiles-tab')[1].element as HTMLElement;
    second.getBoundingClientRect = () =>
      ({ left: 320, right: 420, top: 0, bottom: 28, width: 100, height: 28 }) as DOMRect;

    await wrapper.setProps({ activeKey: 'b' });
    await nextTick();

    expect(strip.scrollLeft).toBe(120);
  });
});
