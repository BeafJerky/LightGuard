//! 仓库设置表单与 daemon.json 请求体之间的纯函数转换。
//! 加速源是行列表（URL + 本机备注），代理是单个服务器 + 可选账号密码；
//! 空白表示“保持原值”，只有关闭开关或显式输入才会改动对应键。

import type { RegistryConfig } from './api';

export type ProxyKey = 'httpProxy' | 'httpsProxy' | 'noProxy';

export interface MirrorRow {
  url: string;
  /** 备注只保存在本机，不写入 daemon.json。 */
  note: string;
}

export interface MirrorDraft {
  payload: DaemonPayload;
  errors: string[];
  mirrors: string[];
}

export interface ProxyForm {
  enabled: boolean;
  server: string;
  account: string;
  password: string;
  noProxy: string;
}

export interface ProxyDraft {
  payload: DaemonPayload;
  errors: string[];
}

/** 与后端 `docker::daemon::MAX_MIRRORS` 一致。 */
export const MAX_MIRRORS = 10;

/** 与后端 `docker::daemon::DaemonPayload` 一致（serde camelCase）。 */
export interface DaemonPayload {
  mirrors?: string[];
  httpProxy?: string;
  httpsProxy?: string;
  noProxy?: string;
  clear?: string[];
}

/** 与后端 `validate::MAX_URL` 一致：代理与 NO_PROXY 上限 2048 个字符。 */
const MAX_VALUE = 2048;
// 与后端 `proxy_url` 同义：必须有协议、协议后非空、无空白与控制字符（含 DEL 与 C1）。
const HTTP_URL = /^https?:\/\/[^\s\u0000-\u001f\u007f-\u009f]+$/;
const NO_PROXY = /^[A-Za-z0-9._,\-:/*\[\]]+$/;
const USER_INFO = /^https?:\/\/[^/@]*@/;
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

export function isHttpUrl(value: string): boolean {
  return value.length <= MAX_VALUE && HTTP_URL.test(value);
}

export function isNoProxy(value: string): boolean {
  return value.length <= MAX_VALUE && NO_PROXY.test(value);
}

/** 备注按 URL 索引；空备注不落盘。 */
export function mirrorRowsFromConfig(
  config: RegistryConfig,
  notes: Record<string, string> = {},
): MirrorRow[] {
  return config.mirrors.map((url) => ({ url, note: notes[url] ?? '' }));
}

export function buildMirrorsPayload(
  rows: MirrorRow[],
  options: { mirrorsKnown?: boolean } = {},
): MirrorDraft {
  const mirrorsKnown = options.mirrorsKnown ?? true;
  const mirrors: string[] = [];
  const errors: string[] = [];

  for (const row of rows) {
    const url = row.url.trim();
    if (!url) continue;
    if (!isHttpUrl(url)) {
      errors.push(`加速源 ${url} 必须是 http:// 或 https:// URL`);
      continue;
    }
    if (!mirrors.includes(url)) mirrors.push(url);
  }

  if (mirrors.length > MAX_MIRRORS) {
    errors.push(`最多保留 ${MAX_MIRRORS} 个加速源，请先删除多余的条目`);
  }
  // 无法读取现有加速源时，空列表不能当成“清空加速源”。
  if (!mirrorsKnown && mirrors.length === 0) {
    errors.push('需要系统授权后才能读取现有加速源');
  }

  const payload: DaemonPayload = {};
  if (mirrorsKnown || mirrors.length > 0) payload.mirrors = mirrors;
  return { payload, errors, mirrors };
}

export function proxyFormFromConfig(config: RegistryConfig): ProxyForm {
  // 读回的代理值含凭据时会被清空，此时不能回填；HTTP/HTTPS 不一致时也留空。
  const same = config.httpProxy && config.httpProxy === config.httpsProxy;
  return {
    enabled: config.proxyConfigured || Boolean(config.httpProxy || config.httpsProxy),
    server: same ? config.httpProxy : '',
    account: '',
    password: '',
    noProxy: config.noProxy
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean)
      .join('\n'),
  };
}

/** 表单里的多行不代理域名 → daemon.json 的逗号分隔值。 */
export function joinNoProxy(text: string): string {
  return text
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean)
    .join(',');
}

/** 账号密码写进代理 URL 的 userinfo：密码只以密文输入，不回显。 */
export function withCredentials(server: string, account: string, password: string): string {
  if (!account) return server;
  const scheme = server.slice(0, server.indexOf('://') + 3);
  const authority = server.slice(scheme.length);
  const user = encodeURIComponent(account);
  const secret = password ? `:${encodeURIComponent(password)}` : '';
  return `${scheme}${user}${secret}@${authority}`;
}

export function buildProxyPayload(
  form: ProxyForm,
  options: { configured: boolean; currentNoProxy: string },
): ProxyDraft {
  const errors: string[] = [];
  const noProxy = joinNoProxy(form.noProxy);
  if (noProxy && !isNoProxy(noProxy)) {
    errors.push('不代理域名只能包含主机名、域名与 CIDR');
  }

  if (!form.enabled) {
    if (!options.configured) {
      errors.push('当前未配置代理，无需保存');
      return { payload: {}, errors };
    }
    return {
      payload: { clear: ['httpProxy', 'httpsProxy', 'noProxy'] },
      errors,
    };
  }

  const server = form.server.trim();
  const account = form.account.trim();
  const password = form.password;
  if (password && !account) errors.push('填写账号后才能设置密码');

  if (!server) {
    if (account || password) errors.push('填写代理服务器后才能设置账号与密码');
    if (!options.configured) {
      errors.push('请填写代理服务器');
      return { payload: {}, errors };
    }
    // 已有代理且留空服务器：保持原值，只允许单独修改不代理域名。
    const payload: DaemonPayload = {};
    if (noProxy && noProxy !== options.currentNoProxy) payload.noProxy = noProxy;
    if (!payload.noProxy) errors.push('没有需要保存的更改');
    return { payload, errors };
  }

  if (!isHttpUrl(server)) {
    errors.push('代理服务器必须是 http:// 或 https:// URL');
  }
  if (USER_INFO.test(server)) {
    errors.push('服务器地址不要包含账号密码，请分别填写');
  }
  if (CONTROL.test(account) || CONTROL.test(password)) {
    errors.push('账号与密码不能包含控制字符');
  }

  const payload: DaemonPayload = {};
  if (errors.length === 0) {
    const target = withCredentials(server, account, password);
    payload.httpProxy = target;
    payload.httpsProxy = target;
    if (noProxy) payload.noProxy = noProxy;
  }
  return { payload, errors };
}
