//! 仓库设置表单与 daemon.json 请求体之间的纯函数转换。
//! 空白表示“保持原值”；只有显式清除或输入新值才会改动对应键。

import type { RegistryConfig } from './api';

export type ProxyKey = 'httpProxy' | 'httpsProxy' | 'noProxy';

export interface SettingsForm {
  mirrors: string;
  httpProxy: string;
  httpsProxy: string;
  noProxy: string;
  /** 用户点击「清除」的代理键；输入新值会把它移出。 */
  clear: ProxyKey[];
}

/** 与后端 `docker::daemon::DaemonPayload` 一致（serde camelCase）。 */
export interface DaemonPayload {
  mirrors?: string[];
  httpProxy?: string;
  httpsProxy?: string;
  noProxy?: string;
  clear?: string[];
}

export interface SettingsDraft {
  payload: DaemonPayload;
  errors: string[];
}

/** 与后端 `validate::MAX_URL` 一致：代理与 NO_PROXY 上限 2048 个字符。 */
const MAX_VALUE = 2048;
// 与后端 `proxy_url` 同义：必须有协议、协议后非空、无空白与控制字符（含 DEL 与 C1）。
const HTTP_URL = /^https?:\/\/[^\s\u0000-\u001f\u007f-\u009f]+$/;
const NO_PROXY = /^[A-Za-z0-9._,\-:/*\[\]]+$/;

export function isHttpUrl(value: string): boolean {
  return value.length <= MAX_VALUE && HTTP_URL.test(value);
}

export function isNoProxy(value: string): boolean {
  return value.length <= MAX_VALUE && NO_PROXY.test(value);
}

export function formFromConfig(config: RegistryConfig): SettingsForm {
  return {
    mirrors: config.mirrors.join('\n'),
    httpProxy: config.httpProxy,
    httpsProxy: config.httpsProxy,
    noProxy: config.noProxy,
    clear: [],
  };
}

export function parseMirrors(text: string): { mirrors: string[]; invalid: string[] } {
  const mirrors: string[] = [];
  const invalid: string[] = [];
  for (const line of text.split(/\r?\n/)) {
    const value = line.trim();
    if (!value) continue;
    if (!isHttpUrl(value)) {
      invalid.push(value);
      continue;
    }
    if (!mirrors.includes(value)) mirrors.push(value);
  }
  return { mirrors, invalid };
}

export function buildDaemonPayload(
  form: SettingsForm,
  options: { mirrorsKnown?: boolean } = {},
): SettingsDraft {
  const mirrorsKnown = options.mirrorsKnown ?? true;
  const parsed = parseMirrors(form.mirrors);
  const errors = parsed.invalid.map(
    (value) => `镜像加速源 ${value} 必须是 http:// 或 https:// URL`,
  );

  const payload: DaemonPayload = {};
  // 无法读取现有加速源时，空输入不能当成“清空加速源”。
  if (mirrorsKnown || form.mirrors.trim().length > 0) {
    payload.mirrors = parsed.mirrors;
  }

  const rows: Array<[ProxyKey, string, string]> = [
    ['httpProxy', 'HTTP 代理', form.httpProxy],
    ['httpsProxy', 'HTTPS 代理', form.httpsProxy],
    ['noProxy', 'NO_PROXY', form.noProxy],
  ];
  const clear: string[] = [];
  for (const [key, label, raw] of rows) {
    const value = raw.trim();
    if (value) {
      const valid = key === 'noProxy' ? isNoProxy(value) : isHttpUrl(value);
      if (!valid) {
        errors.push(
          key === 'noProxy'
            ? 'NO_PROXY 只能包含主机名、域名与 CIDR'
            : `${label}必须是 http:// 或 https:// URL`,
        );
      }
      payload[key] = value;
      continue;
    }
    if (form.clear.includes(key)) clear.push(key);
  }
  if (clear.length > 0) payload.clear = clear;
  return { payload, errors };
}
