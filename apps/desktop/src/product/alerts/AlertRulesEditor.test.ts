// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import { LgSwitch } from '@lightguard/lightui';
import AlertRulesEditor from './AlertRulesEditor.vue';
import { DEFAULT_ALERT_SETTINGS, normalizeAlertSettings } from './settings';
import type { AlertSettings } from '../types';

function factory() {
  return mount(AlertRulesEditor, {
    props: {
      settings: normalizeAlertSettings(DEFAULT_ALERT_SETTINGS),
      servers: [
        { id: 'srv_1', name: '生产机' },
        { id: 'srv_2', name: '测试机' },
      ],
    },
  });
}

function lastUpdate(wrapper: ReturnType<typeof factory>): AlertSettings {
  const events = wrapper.emitted('update') ?? [];
  return events[events.length - 1]?.[0] as AlertSettings;
}

describe('AlertRulesEditor', () => {
  it('renders default thresholds and server options', () => {
    const wrapper = factory();
    expect((wrapper.find('[aria-label="CPU 阈值（%）"]').element as HTMLInputElement).value).toBe('90');
    expect(
      (wrapper.find('[aria-label="磁盘持续秒数"]').element as HTMLInputElement).value,
    ).toBe('600');
    expect(wrapper.findAll('#rules-target option')).toHaveLength(3);
    wrapper.unmount();
  });

  it('clamps edited values before emitting', async () => {
    const wrapper = factory();
    const cpu = wrapper.find('[aria-label="CPU 阈值（%）"]');
    await cpu.setValue('120');
    await cpu.trigger('change');
    expect(lastUpdate(wrapper).rules.cpu.threshold).toBe(100);
    wrapper.unmount();
  });

  it('toggles login monitoring independently', async () => {
    const wrapper = factory();
    const switches = wrapper.findAllComponents(LgSwitch);
    await switches[1].vm.$emit('update:value', false);
    const next = lastUpdate(wrapper);
    expect(next.rules.login.enabled).toBe(false);
    expect(next.rules.hostkey.enabled).toBe(true);
    wrapper.unmount();
  });

  it('writes and clears a per-server override', async () => {
    const wrapper = factory();
    await wrapper.find('#rules-target').setValue('srv_1');
    const cpu = wrapper.find('[aria-label="CPU 阈值（%）"]');
    await cpu.setValue('70');
    await cpu.trigger('change');
    const next = lastUpdate(wrapper);
    expect(next.perServer.srv_1?.cpu?.threshold).toBe(70);
    expect(next.perServer.srv_1?.memory?.threshold).toBe(90);

    await wrapper.find('.rules-editor__reset').trigger('click');
    expect(lastUpdate(wrapper).perServer).toEqual({});
    wrapper.unmount();
  });

  it('clamps every editable field to the backend ranges', async () => {
    const wrapper = factory();
    const edit = async (label: string, value: string) => {
      const input = wrapper.find(`[aria-label="${label}"]`);
      await input.setValue(value);
      await input.trigger('change');
    };

    await edit('CPU 持续秒数', '5');
    expect(lastUpdate(wrapper).rules.cpu.sustainSeconds).toBe(60);
    await edit('磁盘阈值（%）', '120');
    expect(lastUpdate(wrapper).rules.disk.threshold).toBe(100);
    await edit('负载倍数', '99');
    expect(lastUpdate(wrapper).rules.load.multiplier).toBe(10);
    await edit('负载持续秒数', '99999');
    expect(lastUpdate(wrapper).rules.load.sustainSeconds).toBe(3600);
    await edit('离线宽限秒数', '5');
    expect(lastUpdate(wrapper).rules.offline.graceSeconds).toBe(30);
    wrapper.unmount();
  });

  it('emits a fresh AlertSettings tree without touching the given props', async () => {
    const settings = normalizeAlertSettings(DEFAULT_ALERT_SETTINGS);
    const wrapper = mount(AlertRulesEditor, {
      props: { settings, servers: [{ id: 'srv_1', name: '生产机' }] },
    });
    const cpu = wrapper.find('[aria-label="CPU 阈值（%）"]');
    await cpu.setValue('70');
    await cpu.trigger('change');

    const next = lastUpdate(wrapper);
    expect(settings.rules.cpu.threshold).toBe(90);
    expect(settings.perServer).toEqual({});
    expect(next.rules).not.toBe(settings.rules);
    expect(next.rules.cpu).not.toBe(settings.rules.cpu);
    expect(Object.keys(next).sort()).toEqual(['perServer', 'rules']);
    wrapper.unmount();
  });

  it('marks overridden servers and keeps global edits away from them', async () => {
    const wrapper = factory();
    await wrapper.find('#rules-target').setValue('srv_1');
    await wrapper.find('[aria-label="CPU 阈值（%）"]').setValue('70');
    await wrapper.find('[aria-label="CPU 阈值（%）"]').trigger('change');

    expect(wrapper.find('#rules-target').text()).toContain('生产机（独立）');
    expect(wrapper.text()).toContain('这台服务器使用独立规则');

    await wrapper.find('#rules-target').setValue('');
    await wrapper.find('[aria-label="内存阈值（%）"]').setValue('80');
    await wrapper.find('[aria-label="内存阈值（%）"]').trigger('change');

    const next = lastUpdate(wrapper);
    expect(next.rules.memory.threshold).toBe(80);
    expect(next.perServer.srv_1?.memory?.threshold).toBe(90);
    wrapper.unmount();
  });
});
