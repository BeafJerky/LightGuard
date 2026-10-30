import { describe, expect, it } from 'vitest';
import type { RegistryConfig } from './api';
import {
  buildMirrorsPayload,
  buildProxyPayload,
  joinNoProxy,
  mirrorRowsFromConfig,
  proxyFormFromConfig,
  withCredentials,
} from './settings';

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

describe('mirrorRowsFromConfig', () => {
  it('按 URL 带出本机备注', () => {
    const rows = mirrorRowsFromConfig(config, { 'https://mirror.example.com': '主用' });
    expect(rows).toEqual([
      { url: 'https://mirror.example.com', note: '主用' },
      { url: 'http://mirror2.example.com', note: '' },
    ]);
  });
});

describe('buildMirrorsPayload', () => {
  it('拆分空行、去重并保留顺序', () => {
    const draft = buildMirrorsPayload([
      { url: ' https://a.example.com ', note: '' },
      { url: '', note: '只有备注' },
      { url: 'http://b.example.com', note: '' },
      { url: 'https://a.example.com', note: '' },
    ]);
    expect(draft.errors).toEqual([]);
    expect(draft.payload.mirrors).toEqual(['https://a.example.com', 'http://b.example.com']);
  });

  it('标记非 http(s) 地址并给出中文提示', () => {
    const draft = buildMirrorsPayload([{ url: 'ftp://a', note: '' }]);
    expect(draft.errors).toEqual(['加速源 ftp://a 必须是 http:// 或 https:// URL']);
  });

  it('超过十个加速源时阻止保存', () => {
    const rows = Array.from({ length: 11 }, (_, index) => ({
      url: `https://mirror-${index}.example.com`,
      note: '',
    }));
    expect(buildMirrorsPayload(rows).errors[0]).toContain('最多保留 10 个');
  });

  it('无法读取现有加速源时，空列表不能当成清空', () => {
    const draft = buildMirrorsPayload([{ url: '', note: '' }], { mirrorsKnown: false });
    expect(draft.payload).not.toHaveProperty('mirrors');
    expect(draft.errors).toEqual(['需要系统授权后才能读取现有加速源']);
  });

  it('无法读取但用户填写了地址时仍然可以覆盖', () => {
    const draft = buildMirrorsPayload([{ url: 'https://a.example.com', note: '' }], {
      mirrorsKnown: false,
    });
    expect(draft.payload.mirrors).toEqual(['https://a.example.com']);
    expect(draft.errors).toEqual([]);
  });
});

describe('proxyFormFromConfig', () => {
  it('回填可读且一致的代理服务器，多行展示不代理域名', () => {
    const form = proxyFormFromConfig({
      ...config,
      httpProxy: 'http://proxy:3128',
      httpsProxy: 'http://proxy:3128',
      noProxy: 'localhost,127.0.0.1',
    });
    expect(form.enabled).toBe(true);
    expect(form.server).toBe('http://proxy:3128');
    expect(form.noProxy).toBe('localhost\n127.0.0.1');
  });

  it('HTTP/HTTPS 不一致或含凭据被隐藏时留空服务器', () => {
    const form = proxyFormFromConfig({
      ...config,
      proxyConfigured: true,
      httpProxy: 'http://a:3128',
      httpsProxy: 'http://b:3128',
    });
    expect(form.enabled).toBe(true);
    expect(form.server).toBe('');
  });
});

describe('withCredentials', () => {
  it('把账号密码编码进 userinfo', () => {
    expect(withCredentials('http://proxy:3128', 'u@x', 'p:w')).toBe(
      'http://u%40x:p%3Aw@proxy:3128',
    );
    expect(withCredentials('https://proxy:3128', '', '')).toBe('https://proxy:3128');
  });
});

describe('joinNoProxy', () => {
  it('多行合并为逗号分隔', () => {
    expect(joinNoProxy(' localhost \n\n10.0.0.0/8\n')).toBe('localhost,10.0.0.0/8');
  });
});

describe('buildProxyPayload', () => {
  it('关闭代理时清除三个代理键', () => {
    const draft = buildProxyPayload(
      { enabled: false, server: '', account: '', password: '', noProxy: '' },
      { configured: true, currentNoProxy: 'localhost' },
    );
    expect(draft.errors).toEqual([]);
    expect(draft.payload.clear).toEqual(['httpProxy', 'httpsProxy', 'noProxy']);
  });

  it('未配置代理时直接关闭不产生空操作', () => {
    const draft = buildProxyPayload(
      { enabled: false, server: '', account: '', password: '', noProxy: '' },
      { configured: false, currentNoProxy: '' },
    );
    expect(draft.payload).toEqual({});
    expect(draft.errors).toEqual(['当前未配置代理，无需保存']);
  });

  it('开启代理时 HTTP/HTTPS 同时写入合成后的地址', () => {
    const draft = buildProxyPayload(
      {
        enabled: true,
        server: 'http://proxy:3128',
        account: 'user',
        password: 'pa:ss',
        noProxy: 'localhost\n10.0.0.0/8',
      },
      { configured: false, currentNoProxy: '' },
    );
    expect(draft.errors).toEqual([]);
    expect(draft.payload.httpProxy).toBe('http://user:pa%3Ass@proxy:3128');
    expect(draft.payload.httpsProxy).toBe('http://user:pa%3Ass@proxy:3128');
    expect(draft.payload.noProxy).toBe('localhost,10.0.0.0/8');
  });

  it('服务器带凭据、只填密码或协议不合法时给出提示', () => {
    const withUser = buildProxyPayload(
      { enabled: true, server: 'http://u:p@proxy:3128', account: '', password: '', noProxy: '' },
      { configured: false, currentNoProxy: '' },
    );
    expect(withUser.errors).toEqual(['服务器地址不要包含账号密码，请分别填写']);

    const onlyPassword = buildProxyPayload(
      { enabled: true, server: 'http://proxy:3128', account: '', password: 'p', noProxy: '' },
      { configured: false, currentNoProxy: '' },
    );
    expect(onlyPassword.errors).toContain('填写账号后才能设置密码');

    const badScheme = buildProxyPayload(
      { enabled: true, server: 'socks5://proxy:1080', account: '', password: '', noProxy: '' },
      { configured: false, currentNoProxy: '' },
    );
    expect(badScheme.errors).toEqual(['代理服务器必须是 http:// 或 https:// URL']);
  });

  it('已有代理留空服务器时只更新不代理域名', () => {
    const draft = buildProxyPayload(
      { enabled: true, server: '', account: '', password: '', noProxy: 'localhost\n.example' },
      { configured: true, currentNoProxy: 'localhost' },
    );
    expect(draft.errors).toEqual([]);
    expect(draft.payload).toEqual({ noProxy: 'localhost,.example' });
  });

  it('已有代理且没有任何改动时拒绝空保存', () => {
    const draft = buildProxyPayload(
      { enabled: true, server: '', account: '', password: '', noProxy: 'localhost' },
      { configured: true, currentNoProxy: 'localhost' },
    );
    expect(draft.payload).toEqual({});
    expect(draft.errors).toEqual(['没有需要保存的更改']);
  });
});
