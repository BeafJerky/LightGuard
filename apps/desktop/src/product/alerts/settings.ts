import type {
  AlertRules,
  AlertRuleOverride,
  AlertSettings,
  LoadRule,
  OfflineRule,
  PermitMinutes,
  ResourceRule,
  ToggleRule,
} from '../types';

export const PERMIT_OPTIONS: PermitMinutes[] = [5, 15, 30];

export const DEFAULT_ALERT_RULES: AlertRules = {
  cpu: { enabled: true, threshold: 90, sustainSeconds: 180 },
  memory: { enabled: true, threshold: 90, sustainSeconds: 180 },
  disk: { enabled: true, threshold: 90, sustainSeconds: 600 },
  load: { enabled: true, multiplier: 2, sustainSeconds: 300 },
  offline: { enabled: true, graceSeconds: 60 },
  hostkey: { enabled: true },
  login: { enabled: true },
};

export const DEFAULT_ALERT_SETTINGS: AlertSettings = {
  rules: DEFAULT_ALERT_RULES,
  perServer: {},
};

const RULE_KEYS = ['cpu', 'memory', 'disk', 'load', 'offline', 'hostkey', 'login'] as const;

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function readBool(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function readInt(value: unknown, min: number, max: number, fallback: number): number {
  const number = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(max, Math.max(min, Math.round(number)));
}

function readFloat(value: unknown, min: number, max: number, fallback: number): number {
  const number = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(max, Math.max(min, Math.round(number * 10) / 10));
}

function normalizeResource(value: unknown, fallback: ResourceRule): ResourceRule {
  const raw = record(value);
  return {
    enabled: readBool(raw.enabled, fallback.enabled),
    threshold: readInt(raw.threshold, 50, 100, fallback.threshold),
    sustainSeconds: readInt(raw.sustainSeconds, 60, 3600, fallback.sustainSeconds),
  };
}

function normalizeLoad(value: unknown, fallback: LoadRule): LoadRule {
  const raw = record(value);
  return {
    enabled: readBool(raw.enabled, fallback.enabled),
    multiplier: readFloat(raw.multiplier, 1, 10, fallback.multiplier),
    sustainSeconds: readInt(raw.sustainSeconds, 60, 3600, fallback.sustainSeconds),
  };
}

function normalizeOffline(value: unknown, fallback: OfflineRule): OfflineRule {
  const raw = record(value);
  return {
    enabled: readBool(raw.enabled, fallback.enabled),
    graceSeconds: readInt(raw.graceSeconds, 30, 300, fallback.graceSeconds),
  };
}

function normalizeToggle(value: unknown, fallback: ToggleRule): ToggleRule {
  return { enabled: readBool(record(value).enabled, fallback.enabled) };
}

export function normalizeAlertRules(
  value: unknown,
  fallback: AlertRules = DEFAULT_ALERT_RULES,
): AlertRules {
  const raw = record(value);
  return {
    cpu: normalizeResource(raw.cpu, fallback.cpu),
    memory: normalizeResource(raw.memory, fallback.memory),
    disk: normalizeResource(raw.disk, fallback.disk),
    load: normalizeLoad(raw.load, fallback.load),
    offline: normalizeOffline(raw.offline, fallback.offline),
    hostkey: normalizeToggle(raw.hostkey, fallback.hostkey),
    login: normalizeToggle(raw.login, fallback.login),
  };
}

export function normalizeAlertSettings(value: unknown): AlertSettings {
  const raw = record(value);
  const rules = normalizeAlertRules(raw.rules);
  const perServer: Record<string, AlertRuleOverride> = {};
  for (const [serverId, override] of Object.entries(record(raw.perServer))) {
    if (!serverId) continue;
    const normalized = normalizeAlertRules(override, rules);
    const partial: AlertRuleOverride = {};
    const present = record(override);
    for (const key of RULE_KEYS) {
      if (key in present) Object.assign(partial, { [key]: normalized[key] });
    }
    if (Object.keys(partial).length) perServer[serverId] = partial;
  }
  return { rules, perServer };
}

export function normalizePermitMinutes(value: unknown): PermitMinutes {
  return value === 5 || value === 15 || value === 30 ? value : 15;
}

export function normalizePermitSettings(value: unknown): { defaultMinutes: PermitMinutes } {
  return { defaultMinutes: normalizePermitMinutes(record(value).defaultMinutes) };
}

export function effectiveRules(settings: AlertSettings, serverId: string): AlertRules {
  const override = settings.perServer[serverId];
  return override ? normalizeAlertRules(override, settings.rules) : settings.rules;
}
