import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import DesktopWindow from './DesktopWindow.vue';
import type { DesktopWindowState } from './useDesktopWindows';
import type { Server } from '../types';

const server = { id: 'srv-1', name: 'Hermers', status: 'online' } as unknown as Server;

const PointerLike = typeof PointerEvent !== 'undefined' ? PointerEvent : MouseEvent;

function makeWin(over: Partial<DesktopWindowState> = {}): DesktopWindowState {
  return {
    app: 'about',
    x: 40,
    y: 60,
    w: 800,
    h: 500,
    z: 1,
    minimized: false,
    maximized: false,
    restore: null,
    ...over,
  };
}

function rect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

function mountWindow(win: DesktopWindowState) {
  const host = document.createElement('div');
  document.body.appendChild(host);
  const wrapper = mount(DesktopWindow, {
    attachTo: host,
    props: { win, active: true, server, session: 's1', startedAt: '2026-01-01', topInset: 0, bottomInset: 54 },
    global: { stubs: { DesktopAppHost: true, LgIcon: true } },
  });
  (wrapper.element as HTMLElement).getBoundingClientRect = () => rect(0, 0, 1440, 846);
  // VTU 会在 attachTo 容器与组件根节点之间插入一层容器，桩要打在真实 parent 上
  (wrapper.element.parentElement as HTMLElement).getBoundingClientRect = () => rect(0, 0, 1440, 900);
  return wrapper;
}

function pointerDown(el: Element, x: number, y: number) {
  el.dispatchEvent(new PointerLike('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
}

function pointerMove(x: number, y: number) {
  window.dispatchEvent(new PointerLike('pointermove', { bubbles: true, clientX: x, clientY: y }));
}

function pointerUp() {
  window.dispatchEvent(new PointerLike('pointerup', { bubbles: true }));
}

function doubleClick(el: Element) {
  el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('DesktopWindow drag restore', () => {
  it('restores a maximized window on title pointerdown and follows the drag', () => {
    vi.spyOn(performance, 'now').mockReturnValue(1000);
    const wrapper = mountWindow(makeWin({ maximized: true, restore: { x: 40, y: 60, w: 800, h: 500 } }));
    const bar = wrapper.find('.lgdesk-win__bar');

    pointerDown(bar.element, 720, 20);
    // 按下本身不还原，等待拖动越过阈值
    expect(wrapper.emitted('maximize')).toBeUndefined();
    expect(wrapper.emitted('move')).toBeUndefined();

    pointerMove(722, 22);
    expect(wrapper.emitted('maximize')).toBeUndefined();

    pointerMove(800, 120);
    expect(wrapper.emitted('maximize')).toHaveLength(1);
    // 保持指针横向比例 0.5 与标题栏抓取偏移 20px
    expect(wrapper.emitted('move')?.[0]).toEqual([400, 100]);

    pointerMove(900, 200);
    const moves = wrapper.emitted('move') ?? [];
    expect(moves[moves.length - 1]).toEqual([500, 180]);
    pointerUp();

    const count = moves.length;
    pointerMove(900, 260);
    expect(wrapper.emitted('move')).toHaveLength(count);
    wrapper.unmount();
  });

  it('keeps a maximized window when the title bar is only clicked', () => {
    vi.spyOn(performance, 'now').mockReturnValue(1000);
    const wrapper = mountWindow(makeWin({ maximized: true, restore: { x: 40, y: 60, w: 800, h: 500 } }));
    const bar = wrapper.find('.lgdesk-win__bar');

    pointerDown(bar.element, 720, 20);
    pointerUp();
    expect(wrapper.emitted('maximize')).toBeUndefined();
    expect(wrapper.emitted('move')).toBeUndefined();
    wrapper.unmount();
  });

  it('ignores a double click right after drag restore but honors it later', () => {
    const now = vi.spyOn(performance, 'now').mockReturnValue(1000);
    const wrapper = mountWindow(makeWin({ maximized: true, restore: { x: 40, y: 60, w: 800, h: 500 } }));
    const bar = wrapper.find('.lgdesk-win__bar');

    pointerDown(bar.element, 720, 20);
    pointerMove(800, 120);
    pointerUp();
    expect(wrapper.emitted('maximize')).toHaveLength(1);

    now.mockReturnValue(1500);
    doubleClick(bar.element);
    expect(wrapper.emitted('maximize')).toHaveLength(1);

    now.mockReturnValue(1700);
    doubleClick(bar.element);
    expect(wrapper.emitted('maximize')).toHaveLength(2);
    wrapper.unmount();
  });

  it('keeps plain double click maximize for restored windows', () => {
    vi.spyOn(performance, 'now').mockReturnValue(5000);
    const wrapper: VueWrapper = mountWindow(makeWin());
    const bar = wrapper.find('.lgdesk-win__bar');

    pointerDown(bar.element, 300, 80);
    expect(wrapper.emitted('maximize')).toBeUndefined();
    pointerUp();

    doubleClick(bar.element);
    expect(wrapper.emitted('maximize')).toHaveLength(1);
    wrapper.unmount();
  });
});
