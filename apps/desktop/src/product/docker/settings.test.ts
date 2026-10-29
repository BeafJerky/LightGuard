import { describe, expect, it } from 'vitest';
import type { RegistryConfig } from './api';
import { buildDaemonPayload, formFromConfig, parseMirrors } from './settings';

const config: RegistryConfig = {
  mirrors: ['https://mirror.example.com', 'http://mirror2.example.com'],
  httpProxy: '',
  httpsProxy: '',
  noProxy: '',
  proxyConfigured: false,
  proxySupported: true,
  readable: true,
  invalid: false,
  note: '',
};

describe('parseMirrors', () => {
  it('拆除空行与重复地址，保留顺序', () => {
    expect(
      parseMirrors('https://a.example.com\n\n http://b.example.com \nhttps://a.example.com').mirrors,
    ).toEqual(['https://a.example.com', 'http://b.example.com']);
  });

  it('标记非 http(s) 地址', () => {
    expect(parseMirrors('ftp://a\nhttps://b').invalid).toEqual(['ftp://a']);
  });
});

describe('buildDaemonPayload', () => {
  it('空白代理不发送，留空保持已有配置', () => {
    const form = formFromConfig(config);
    form.mirrors = '';
    const draft = buildDaemonPayload(form);
    expect(draft.payload.mirrors).toEqual([]);
    expect(draft.payload).not.toHaveProperty('httpProxy');
    expect(draft.payload.clear).toBeUndefined();
  });

  it('显式清除只发送对应键，输入新值会取消清除', () => {
    const form = formFromConfig(config);
    form.clear = ['httpProxy', 'noProxy'];
    form.noProxy = 'localhost,127.0.0.1';
    const draft = buildDaemonPayload(form);
    expect(draft.payload.clear).toEqual(['httpProxy']);
    expect(draft.payload.noProxy).toBe('localhost,127.0.0.1');
  });

  it('无法读取加速源时留空保持不变', () => {
    const form = formFromConfig(config);
    form.mirrors = '';
    expect(buildDaemonPayload(form, { mirrorsKnown: false }).payload).not.toHaveProperty('mirrors');
  });

  it('拒绝非法代理地址并给出中文提示', () => {
    const form = formFromConfig(config);
    form.httpsProxy = 'https://';
    expect(buildDaemonPayload(form).errors).toEqual(['HTTPS 代理必须是 http:// 或 https:// URL']);
  });
});
