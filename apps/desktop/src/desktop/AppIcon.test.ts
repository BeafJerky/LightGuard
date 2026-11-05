import { describe, expect, it } from 'vitest';
import { computed } from 'vue';
import { mount } from '@vue/test-utils';
import { LG_SCOPE_KEY, themes } from '@lightguard/lightui';
import type { ThemeScopeContext } from '@lightguard/lightui';
import AppIcon from './AppIcon.vue';

function mountIcon(themeId: keyof typeof themes | null, props: Record<string, unknown> = {}) {
  const provide = themeId
    ? {
        [LG_SCOPE_KEY as symbol]: computed<ThemeScopeContext>(() => ({
          theme: themes[themeId],
          scope: 'desktop',
          portalEl: null,
        })),
      }
    : {};
  return mount(AppIcon, {
    props: { app: 'about', tile: 'tile-about', size: 46, glyph: 32, ...props },
    global: { provide },
  });
}

describe('AppIcon', () => {
  it('renders the shared glyph inside a scheme B tile for base themes', () => {
    const wrapper = mountIcon('dark');
    const root = wrapper.get('.app-icon');
    expect(root.classes()).toContain('tile-about');
    expect(root.classes()).toContain('app-icon--b');
    expect(root.attributes('style')).toContain('--app-icon-ink');
    expect(root.attributes('style')).toContain('width: 46px');
    expect(root.attributes('style')).toContain('height: 46px');

    const svg = root.get('svg');
    expect(svg.attributes('viewBox')).toBe('0 0 24 24');
    const path = svg.get('path');
    expect(path.attributes('fill')).toBe('currentColor');
    expect(path.attributes('fill-rule')).toBe('evenodd');
  });

  it('keeps the legacy tile recipe for scheme A themes', () => {
    const wrapper = mountIcon('claude');
    const root = wrapper.get('.app-icon');
    expect(root.classes()).toContain('app-icon--a');
    expect(root.attributes('style') ?? '').not.toContain('--app-icon-ink');
  });

  it('falls back to scheme A without a theme scope', () => {
    const wrapper = mountIcon(null);
    expect(wrapper.get('.app-icon').classes()).toContain('app-icon--a');
  });

  it('renders each glyph part with its own fill or stroke mode', () => {
    const wrapper = mountIcon('dark', { app: 'network', tile: 'tile-network' });
    const paths = wrapper.findAll('svg path');
    expect(paths.length).toBeGreaterThan(1);
    expect(paths.some((path) => path.attributes('fill') === 'none')).toBe(true);
    expect(paths.some((path) => path.attributes('stroke') === 'currentColor')).toBe(true);
  });
});
