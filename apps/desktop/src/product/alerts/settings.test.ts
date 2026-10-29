import { describe, expect, it } from 'vitest';
import {
  DEFAULT_ALERT_RULES,
  DEFAULT_ALERT_SETTINGS,
  effectiveRules,
  normalizeAlertSettings,
  normalizePermitSettings,
} from './settings';

describe('alert settings', () => {
  it('defaults every rule', () => {
    const settings = normalizeAlertSettings(undefined);
    expect(settings.rules).toEqual(DEFAULT_ALERT_RULES);
    expect(settings.perServer).toEqual({});
    expect(DEFAULT_ALERT_RULES.disk.sustainSeconds).toBe(600);
  });

  it('clamps and defaults field by field', () => {
    const settings = normalizeAlertSettings({
      rules: {
        cpu: { threshold: 120, sustainSeconds: 10 },
        load: { multiplier: 0.1, sustainSeconds: 99999 },
        offline: { graceSeconds: 5, enabled: false },
        login: { enabled: false },
      },
    });
    expect(settings.rules.cpu.threshold).toBe(100);
    expect(settings.rules.cpu.sustainSeconds).toBe(60);
    expect(settings.rules.cpu.enabled).toBe(true);
    expect(settings.rules.load.multiplier).toBe(1);
    expect(settings.rules.load.sustainSeconds).toBe(3600);
    expect(settings.rules.offline.graceSeconds).toBe(30);
    expect(settings.rules.offline.enabled).toBe(false);
    expect(settings.rules.login.enabled).toBe(false);
    expect(settings.rules.hostkey.enabled).toBe(true);
    expect(settings.rules.memory).toEqual(DEFAULT_ALERT_RULES.memory);
  });

  it('overrides per server over global rules', () => {
    const settings = normalizeAlertSettings({
      rules: { cpu: { threshold: 80 } },
      perServer: { srv_1: { cpu: { threshold: 70 }, load: { multiplier: 3 } } },
    });
    const rules = effectiveRules(settings, 'srv_1');
    expect(rules.cpu.threshold).toBe(70);
    expect(rules.load.multiplier).toBe(3);
    expect(rules.memory.threshold).toBe(90);
    expect(effectiveRules(settings, 'srv_2').cpu.threshold).toBe(80);
  });

  it('fills a partial server override from the global rules', () => {
    const settings = normalizeAlertSettings({
      rules: { cpu: { enabled: false, threshold: 70, sustainSeconds: 120 } },
      perServer: { srv_1: { cpu: { threshold: 95 } } },
    });
    const stored = settings.perServer.srv_1.cpu;
    expect(stored).toBeDefined();
    expect(stored?.threshold).toBe(95);
    expect(stored?.enabled).toBe(false);
    expect(stored?.sustainSeconds).toBe(120);

    const rules = effectiveRules(settings, 'srv_1');
    expect(rules.cpu).toEqual({ enabled: false, threshold: 95, sustainSeconds: 120 });
    expect(settings.rules.cpu.threshold).toBe(70);
  });

  it('returns a fresh rule object when the server has no override', () => {
    const settings = normalizeAlertSettings(undefined);
    const rules = effectiveRules(settings, 'srv_1');
    expect(rules).not.toBe(settings.rules);
    expect(rules).toEqual(settings.rules);

    rules.cpu.threshold = 55;
    rules.login.enabled = false;
    expect(settings.rules.cpu.threshold).toBe(90);
    expect(settings.rules.login.enabled).toBe(true);

    // The module-level defaults are reachable through DEFAULT_ALERT_SETTINGS.
    const shared = effectiveRules(DEFAULT_ALERT_SETTINGS, 'srv_1');
    expect(shared).not.toBe(DEFAULT_ALERT_RULES);
    shared.cpu.threshold = 55;
    shared.login.enabled = false;
    expect(DEFAULT_ALERT_RULES.cpu.threshold).toBe(90);
    expect(DEFAULT_ALERT_RULES.login.enabled).toBe(true);
  });
});

describe('permit settings', () => {
  it('accepts only 5/15/30', () => {
    expect(normalizePermitSettings({ defaultMinutes: 30 }).defaultMinutes).toBe(30);
    expect(normalizePermitSettings({ defaultMinutes: 7 }).defaultMinutes).toBe(15);
    expect(normalizePermitSettings(null).defaultMinutes).toBe(15);
  });
});
