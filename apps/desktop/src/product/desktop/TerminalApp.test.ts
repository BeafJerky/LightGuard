import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// 样式在 SFC 的 scoped block 里，jsdom 不会计算它；这里直接守护源码规则，
// 防止后续重构再次把断开遮罩压回 xterm 链接层之下。
const source = readFileSync(resolve(process.cwd(), 'src/product/desktop/TerminalApp.vue'), 'utf8');

const styleBlock = (selector: string): string => {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = source.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\}`));
  if (!match) throw new Error(`missing style block for ${selector}`);
  return match[1];
};

describe('terminal overlay stacking', () => {
  it('keeps the disconnected overlay above the xterm link layer', () => {
    // xterm 6 的链接层 canvas 为 z-index:2 且 pointer-events:auto，覆盖整个终端画面；
    // 遮罩若没有更高层级，物理点击与 UIA 激活都到不了「重连会话」按钮。
    const overlay = styleBlock('.term-overlay');
    const zIndex = Number(overlay.match(/z-index:\s*(\d+)/)?.[1] ?? 0);
    expect(zIndex).toBeGreaterThanOrEqual(12);
    // 仍低于右键菜单（.term-menu），避免遮挡操作面板。
    expect(zIndex).toBeLessThan(40);
  });
});