// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import BrandMark from './BrandMark.vue';

describe('BrandMark', () => {
  it('renders both tier images so the theme scope can pick one in CSS', () => {
    const wrapper = mount(BrandMark, { props: { size: 38 } });
    const images = wrapper.findAll('img');
    expect(images).toHaveLength(2);

    const light = wrapper.get('img.brand-mark__img--light');
    const dark = wrapper.get('img.brand-mark__img--dark');
    expect(light.attributes('src')).toBeTruthy();
    expect(dark.attributes('src')).toBeTruthy();
    expect(light.attributes('src')).not.toBe(dark.attributes('src'));
    expect(light.attributes('alt')).toBe('');
    expect(dark.attributes('alt')).toBe('');
  });

  it('sizes the mark and keeps it out of the accessibility tree', () => {
    const wrapper = mount(BrandMark, { props: { size: 26, radius: '8px' } });
    const root = wrapper.get('.brand-mark');
    expect(root.attributes('aria-hidden')).toBe('true');
    expect(root.attributes('style')).toContain('width: 26px');
    expect(root.attributes('style')).toContain('height: 26px');
    expect(root.attributes('style')).toContain('border-radius: 8px');
  });

  it('uses the shared small radius by default', () => {
    const wrapper = mount(BrandMark, { props: { size: 24 } });
    expect(wrapper.get('.brand-mark').attributes('style')).toContain(
      'border-radius: var(--lg-radius-sm)',
    );
  });
});
