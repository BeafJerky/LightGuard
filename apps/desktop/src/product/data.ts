import type {
  AuthKind,
  DiskInfo,
  InterfaceInfo,
  LogEntry,
  LogSource,
  LogStatus,
  ProcessInfo,
  Server,
  ServerError,
  ServerMetrics,
  ServerStatus,
} from './types';

export const APP_VERSION = '0.8.0';
export const APP_BUILD = '2026.10.27';
export const HISTORY_POINTS = 120;
export const MAX_LOGS = 80;

export const STATUS_META: Record<ServerStatus, { label: string; tone: string }> = {
  online: { label: '在线', tone: 'success' },
  warning: { label: '需关注', tone: 'warning' },
  error: { label: '连接失败', tone: 'error' },
  offline: { label: '已离线', tone: 'default' },
  connecting: { label: '连接中', tone: 'info' },
};

export const AUTH_LABELS: Record<AuthKind, string> = {
  key: 'SSH 私钥',
  password: '密码',
  agent: 'SSH Agent',
};

export const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
export const rand = (min: number, max: number) => Math.random() * (max - min) + min;
export const round = (value: number, digits = 0) => {
  const p = 10 ** digits;
  return Math.round(value * p) / p;
};
export const uid = (prefix = 'id') =>
  `${prefix}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;

export function historySeries(
  base: number,
  amp = 6,
  min = 0.5,
  max = 100,
  points = HISTORY_POINTS,
) {
  const out: number[] = [];
  let value = clamp(base + rand(-amp, amp), min, max);
  for (let i = 0; i < points; i += 1) {
    value = clamp(value + rand(-amp, amp) * 0.62 + (base - value) * 0.16, min, max);
    out.push(round(value, 1));
  }
  out[out.length - 1] = round(base, 1);
  return out;
}

export function defaultDisks(total = 120, usage = 18): DiskInfo[] {
  return [
    { mount: '/', fs: 'ext4', type: 'SSD', total, used: Math.round((total * usage) / 100) },
    {
      mount: '/data',
      fs: 'xfs',
      type: 'SSD',
      total: Math.round(total * 1.5),
      used: Math.round(total * 0.9),
    },
    { mount: '/boot', fs: 'ext4', type: 'SSD', total: 2, used: 1 },
  ];
}

export function defaultInterfaces(ip: string, metrics: ServerMetrics): InterfaceInfo[] {
  return [
    {
      name: 'eth0',
      ip,
      mac: '00:16:3e:7a:2c:91',
      speed: '1 Gbps',
      down: metrics.down,
      up: metrics.up,
    },
    { name: 'lo', ip: '127.0.0.1', mac: '00:00:00:00:00:00', speed: '10 Gbps', down: 0.1, up: 0.1 },
  ];
}

export function buildProcesses(role: string, metrics: ServerMetrics): ProcessInfo[] {
  const base: ProcessInfo[] = [
    { pid: 1, name: 'systemd', user: 'root', cpu: 0.1, mem: 0.3, state: '运行' },
    { pid: 742, name: 'sshd', user: 'root', cpu: 0.2, mem: 0.4, state: '运行' },
    { pid: 1180, name: 'dockerd', user: 'root', cpu: 1.6, mem: 2.4, state: '运行' },
    { pid: 1194, name: 'containerd', user: 'root', cpu: 0.9, mem: 1.2, state: '运行' },
    { pid: 1290, name: 'chronyd', user: 'chrony', cpu: 0, mem: 0.1, state: '睡眠' },
    { pid: 1522, name: 'rsyslogd', user: 'syslog', cpu: 0.1, mem: 0.2, state: '睡眠' },
    { pid: 1604, name: 'fail2ban-server', user: 'root', cpu: 0.1, mem: 0.3, state: '睡眠' },
    { pid: 1840, name: 'journald', user: 'root', cpu: 0.2, mem: 0.5, state: '睡眠' },
    { pid: 2011, name: 'cron', user: 'root', cpu: 0, mem: 0.1, state: '睡眠' },
    { pid: 2374, name: 'dbus-daemon', user: 'messagebus', cpu: 0, mem: 0.1, state: '睡眠' },
  ];
  const extra: ProcessInfo[] = [];
  if (/PostgreSQL/i.test(role)) {
    extra.push(
      { pid: 3120, name: 'postgres', user: 'postgres', cpu: 22.4, mem: 28.2, state: '运行' },
      { pid: 3190, name: 'postgres: checkpointer', user: 'postgres', cpu: 2.1, mem: 1.8, state: '睡眠' },
      { pid: 3191, name: 'postgres: walwriter', user: 'postgres', cpu: 1.6, mem: 1.2, state: '睡眠' },
      { pid: 3308, name: 'pgbouncer', user: 'postgres', cpu: 1.2, mem: 0.9, state: '运行' },
    );
  } else if (/缓存/i.test(role)) {
    extra.push(
      { pid: 2680, name: 'openresty', user: 'www-data', cpu: 3.4, mem: 1.6, state: '运行' },
      { pid: 2712, name: 'redis-server', user: 'redis', cpu: 2.8, mem: 3.1, state: '运行' },
      { pid: 2744, name: 'consul-agent', user: 'consul', cpu: 0.6, mem: 1, state: '运行' },
    );
  } else if (/备份|存储/i.test(role)) {
    extra.push(
      { pid: 2861, name: 'restic', user: 'backup', cpu: 3.6, mem: 2.2, state: '运行' },
      { pid: 2902, name: 'rsync', user: 'backup', cpu: 1.2, mem: 0.8, state: '睡眠' },
      { pid: 2940, name: 'smartd', user: 'root', cpu: 0.1, mem: 0.2, state: '睡眠' },
    );
  } else if (/CI|构建/i.test(role)) {
    extra.push(
      { pid: 3402, name: 'java', user: 'runner', cpu: 24.8, mem: 18.4, state: '运行' },
      { pid: 3430, name: 'gradle', user: 'runner', cpu: 9.2, mem: 7.1, state: '运行' },
      { pid: 3488, name: 'gitlab-runner', user: 'runner', cpu: 1.4, mem: 1.2, state: '运行' },
    );
  } else {
    extra.push(
      { pid: 2621, name: 'nginx', user: 'www-data', cpu: 3.2, mem: 1.4, state: '运行' },
      { pid: 2874, name: 'node', user: 'deploy', cpu: 17.6, mem: 8.2, state: '运行' },
      { pid: 2891, name: 'node', user: 'deploy', cpu: 6.4, mem: 5.6, state: '睡眠' },
    );
  }
  const all = base.concat(extra);
  const cpuSum = all.reduce((sum, p) => sum + p.cpu, 0) || 1;
  const memSum = all.reduce((sum, p) => sum + p.mem, 0) || 1;
  const cpuScale = metrics.cpu / cpuSum;
  const memScale = clamp(metrics.mem / memSum, 0.35, 1.7);
  return all
    .map((p) => ({
      ...p,
      cpu: round(clamp(p.cpu * cpuScale, 0, 99), 1),
      mem: round(clamp(p.mem * memScale, 0, 99), 1),
    }))
    .sort((a, b) => b.cpu - a.cpu);
}

interface ServerSeed {
  id: string;
  name: string;
  ip: string;
  role?: string;
  port?: number;
  region?: string;
  group?: string;
  tags?: string[];
  note?: string;
  status?: ServerStatus;
  os?: string;
  kernel?: string;
  cpuModel?: string;
  cpuCores?: number;
  cpuFreq?: string;
  memTotal?: number;
  diskTotal?: number;
  user?: string;
  auth?: AuthKind;
  fingerprint?: string;
  uptime?: string;
  bootTime?: string;
  lastSync?: string;
  lastSeen?: string;
  latency?: number;
  load?: [number, number, number];
  temp?: number;
  warning?: string;
  notice?: string;
  error?: ServerError | null;
  metrics?: ServerMetrics;
  disks?: DiskInfo[];
  interfaces?: InterfaceInfo[];
  gateway?: string;
  dns?: string;
  publicIp?: string;
  mtu?: string;
  processes?: ProcessInfo[];
  needsAuth?: boolean;
}

export function makeServer(seed: ServerSeed): Server {
  const metrics = seed.metrics ?? { cpu: 20, mem: 30, disk: 30, down: 5, up: 1 };
  const auth = seed.auth ?? 'key';
  const status = seed.status ?? 'online';
  return {
    id: seed.id,
    name: seed.name,
    role: seed.role ?? '通用服务器',
    ip: seed.ip,
    port: seed.port ?? 22,
    region: seed.region ?? '自定义',
    group: seed.group ?? '未分组',
    tags: seed.tags ?? ['自建'],
    note: seed.note ?? '',
    status,
    os: seed.os ?? 'Ubuntu 24.04.2 LTS',
    kernel: seed.kernel ?? '6.8.0-45-generic',
    arch: 'x86_64',
    cpuModel: seed.cpuModel ?? 'Intel Xeon Platinum 8269CY',
    cpuCores: seed.cpuCores ?? 4,
    cpuFreq: seed.cpuFreq ?? '2.5 GHz',
    memTotal: seed.memTotal ?? 16,
    memType: 'DDR4 ECC',
    diskTotal: seed.diskTotal ?? 200,
    user: seed.user ?? 'deploy',
    auth,
    authLabel: AUTH_LABELS[auth],
    fingerprint: seed.fingerprint ?? 'SHA256:9f2c 4b8e 17d3 a71e 5c62 9b1f 03d8 6e4a',
    uptime: seed.uptime ?? '42 天 3 小时',
    bootTime: seed.bootTime ?? '2026-06-20 04:12',
    timezone: 'Asia/Shanghai',
    lastSync: seed.lastSync ?? '刚刚',
    lastSeen: seed.lastSeen ?? seed.lastSync ?? '刚刚',
    latency: seed.latency ?? 24,
    load: seed.load ?? [0.42, 0.51, 0.48],
    temp: seed.temp ?? 46,
    warning: seed.warning ?? '',
    notice: seed.notice ?? '',
    error: seed.error ?? null,
    needsAuth: seed.needsAuth ?? seed.error?.code === 'AUTH_FAILED',
    metrics,
    history: {
      cpu: historySeries(metrics.cpu, Math.max(2, metrics.cpu * 0.12)),
      mem: historySeries(metrics.mem, 3.5),
      net: historySeries(metrics.down, Math.max(1, metrics.down * 0.25), 0.2, 90),
    },
    disks: seed.disks ?? defaultDisks(seed.diskTotal ?? 200, metrics.disk),
    interfaces: seed.interfaces ?? defaultInterfaces(seed.ip, metrics),
    gateway: seed.gateway ?? '10.0.0.1',
    dns: seed.dns ?? '223.5.5.5 / 100.100.2.136',
    publicIp: seed.publicIp ?? seed.ip,
    mtu: seed.mtu ?? '1500',
    processes: seed.processes ?? buildProcesses(seed.role ?? '通用服务器', metrics),
  };
}

export function buildServers(): Server[] {
  return [
    makeServer({
      id: 'prod-web-01',
      name: 'prod-web-01',
      role: 'Web 应用节点',
      ip: '192.168.10.21',
      region: '上海 · 阿里云',
      group: '生产',
      tags: ['生产', 'Web'],
      status: 'online',
      os: 'Ubuntu 24.04.2 LTS',
      kernel: '6.8.0-45-generic',
      cpuModel: 'Intel Xeon Platinum 8269CY',
      cpuCores: 8,
      memTotal: 16,
      diskTotal: 200,
      user: 'deploy',
      auth: 'key',
      uptime: '128 天 6 小时',
      bootTime: '2026-06-20 04:12',
      lastSync: '12 秒前',
      latency: 24,
      load: [0.42, 0.51, 0.48],
      temp: 48,
      publicIp: '47.100.86.23',
      metrics: { cpu: 42, mem: 63, disk: 71, down: 12.4, up: 3.8 },
      disks: [
        { mount: '/', fs: 'ext4', type: 'SSD', total: 200, used: 142 },
        { mount: '/var/log', fs: 'ext4', type: 'SSD', total: 50, used: 31 },
        { mount: '/data', fs: 'xfs', type: 'NVMe', total: 500, used: 211 },
        { mount: '/boot', fs: 'ext4', type: 'SSD', total: 2, used: 1 },
      ],
      interfaces: [
        { name: 'eth0', ip: '192.168.10.21', mac: '00:16:3e:7a:2c:91', speed: '1 Gbps', down: 12.4, up: 3.8 },
        { name: 'lo', ip: '127.0.0.1', mac: '00:00:00:00:00:00', speed: '10 Gbps', down: 0.1, up: 0.1 },
      ],
    }),
    makeServer({
      id: 'db-core-02',
      name: 'db-core-02',
      role: 'PostgreSQL 主库',
      ip: '10.0.3.14',
      region: '杭州 · 腾讯云',
      group: '生产',
      tags: ['生产', '数据库'],
      status: 'warning',
      os: 'Debian 12.6',
      kernel: '6.1.0-26-amd64',
      cpuModel: 'AMD EPYC 7R32',
      cpuCores: 16,
      memTotal: 64,
      diskTotal: 500,
      user: 'dba',
      auth: 'key',
      uptime: '96 天 18 小时',
      bootTime: '2026-07-21 16:40',
      lastSync: '8 秒前',
      latency: 31,
      load: [6.82, 6.24, 5.71],
      temp: 61,
      warning: '内存使用率 84%，15 分钟负载持续高于 5.5。',
      metrics: { cpu: 78, mem: 84, disk: 66, down: 28.6, up: 9.2 },
      disks: [
        { mount: '/', fs: 'ext4', type: 'SSD', total: 500, used: 330 },
        { mount: '/var/lib/postgresql', fs: 'ext4', type: 'NVMe', total: 2000, used: 1180 },
        { mount: '/var/log', fs: 'ext4', type: 'SSD', total: 100, used: 58 },
      ],
      interfaces: [
        { name: 'bond0', ip: '10.0.3.14', mac: 'fa:16:3e:11:9c:02', speed: '10 Gbps', down: 28.6, up: 9.2 },
        { name: 'lo', ip: '127.0.0.1', mac: '00:00:00:00:00:00', speed: '10 Gbps', down: 0.2, up: 0.2 },
      ],
      gateway: '10.0.3.1',
      dns: '183.60.83.19 / 119.29.29.29',
      publicIp: '129.226.74.18',
    }),
    makeServer({
      id: 'edge-cache-03',
      name: 'edge-cache-03',
      role: '边缘缓存节点',
      ip: '203.0.113.42',
      region: '北京 · 自建机房',
      group: '边缘',
      tags: ['边缘', 'CDN'],
      status: 'online',
      os: 'AlmaLinux 9.4',
      kernel: '5.14.0-427.el9.x86_64',
      cpuModel: 'Intel Xeon E-2288G',
      cpuCores: 4,
      memTotal: 8,
      diskTotal: 120,
      user: 'ops',
      auth: 'key',
      uptime: '64 天 9 小时',
      bootTime: '2026-08-23 11:08',
      lastSync: '6 秒前',
      latency: 18,
      load: [0.21, 0.26, 0.24],
      temp: 39,
      publicIp: '203.0.113.42',
      metrics: { cpu: 18, mem: 41, disk: 38, down: 46.8, up: 21.4 },
      disks: [
        { mount: '/', fs: 'xfs', type: 'SSD', total: 120, used: 46 },
        { mount: '/var/cache/nginx', fs: 'xfs', type: 'SSD', total: 80, used: 22 },
      ],
      interfaces: [
        { name: 'eth0', ip: '203.0.113.42', mac: '3c:ec:ef:52:0a:11', speed: '10 Gbps', down: 46.8, up: 21.4 },
        { name: 'eth1', ip: '10.0.1.12', mac: '3c:ec:ef:52:0a:12', speed: '1 Gbps', down: 0.4, up: 0.2 },
      ],
      gateway: '203.0.113.1',
      dns: '223.5.5.5 / 180.76.76.76',
    }),
    makeServer({
      id: 'staging-app-04',
      name: 'staging-app-04',
      role: '预发环境',
      ip: '10.0.8.77',
      region: '深圳 · 华为云',
      group: '预发',
      tags: ['预发'],
      status: 'offline',
      os: 'Ubuntu 22.04.4 LTS',
      kernel: '5.15.0-116-generic',
      cpuCores: 4,
      memTotal: 8,
      diskTotal: 100,
      user: 'deploy',
      auth: 'key',
      uptime: '—',
      bootTime: '—',
      lastSync: '2 小时前',
      lastSeen: '2 小时前',
      latency: 0,
      load: [0, 0, 0],
      temp: 0,
      error: {
        code: 'HOST_UNREACHABLE',
        message: '目标主机不可达',
        hint: '请确认安全组已放行 22 端口，或检查跳板机与 VPN 状态。',
      },
      metrics: { cpu: 0, mem: 0, disk: 44, down: 0, up: 0 },
      disks: [
        { mount: '/', fs: 'ext4', type: 'SSD', total: 100, used: 44 },
        { mount: '/data', fs: 'ext4', type: 'SSD', total: 200, used: 90 },
      ],
      gateway: '10.0.8.1',
      publicIp: '—',
    }),
    makeServer({
      id: 'backup-vault-05',
      name: 'backup-vault-05',
      role: '备份存储',
      ip: '10.0.12.9',
      region: '成都 · 自建机房',
      group: '存储',
      tags: ['备份', '存储'],
      status: 'online',
      os: 'Rocky Linux 9.3',
      kernel: '5.14.0-362.el9.x86_64',
      cpuModel: 'Intel Xeon E-2288G',
      cpuCores: 2,
      memTotal: 4,
      diskTotal: 4000,
      user: 'backup',
      auth: 'key',
      uptime: '211 天 2 小时',
      bootTime: '2026-03-29 19:26',
      lastSync: '21 秒前',
      latency: 36,
      load: [0.08, 0.11, 0.09],
      temp: 34,
      notice: '备份盘使用率 85%，建议归档冷数据或扩容。',
      metrics: { cpu: 7, mem: 22, disk: 88, down: 3.2, up: 18.6 },
      disks: [
        { mount: '/', fs: 'xfs', type: 'SSD', total: 80, used: 70 },
        { mount: '/mnt/backup', fs: 'xfs', type: 'HDD', total: 4000, used: 3400 },
      ],
      interfaces: [
        { name: 'eno1', ip: '10.0.12.9', mac: 'ac:1f:6b:44:08:21', speed: '10 Gbps', down: 3.2, up: 18.6 },
      ],
      gateway: '10.0.12.1',
      publicIp: '—',
    }),
    makeServer({
      id: 'build-runner-06',
      name: 'build-runner-06',
      role: 'CI 构建节点',
      ip: '10.0.6.31',
      region: '广州 · 自建机房',
      group: 'CI',
      tags: ['CI', '构建'],
      status: 'error',
      os: 'Fedora 40',
      kernel: '6.10.12-200.fc40.x86_64',
      cpuModel: 'AMD Ryzen 9 5950X',
      cpuCores: 8,
      memTotal: 32,
      diskTotal: 400,
      user: 'runner',
      auth: 'password',
      uptime: '—',
      bootTime: '—',
      lastSync: '14 分钟前',
      lastSeen: '14 分钟前',
      latency: 0,
      load: [0, 0, 0],
      temp: 0,
      error: {
        code: 'AUTH_FAILED',
        message: '身份认证失败',
        hint: '凭据可能已轮换，请更新密码或私钥后重试。',
      },
      needsAuth: true,
      metrics: { cpu: 0, mem: 0, disk: 52, down: 0, up: 0 },
      disks: [
        { mount: '/', fs: 'btrfs', type: 'NVMe', total: 400, used: 208 },
        { mount: '/var/lib/gitlab-runner', fs: 'btrfs', type: 'NVMe', total: 800, used: 520 },
      ],
      gateway: '10.0.6.1',
      publicIp: '—',
    }),
  ];
}

export function buildLogs(
  servers: Server[],
  sources?: { web: LogSource; desktop: LogSource },
): LogEntry[] {
  const sourceLabel: { web: LogSource; desktop: LogSource } =
    sources ?? { web: 'Web', desktop: '桌面客户端' };
  const byId = (id: string) => servers.find((server) => server.id === id) ?? servers[0];
  const now = Date.now();
  const make = (
    id: string,
    offsetMs: number,
    action: string,
    status: LogStatus,
    options: {
      duration?: string;
      source?: LogSource;
      code?: string;
      hint?: string;
      session?: string;
    } = {},
  ): LogEntry => {
    const server = byId(id);
    return {
      id: uid('log'),
      ts: now - offsetMs,
      serverId: server.id,
      serverName: server.name,
      ip: server.ip,
      port: server.port,
      action,
      status,
      duration: options.duration ?? '—',
      source: options.source ?? sourceLabel.web,
      code: options.code ?? 'OK',
      auth: server.authLabel,
      client: `LightGuard Web ${APP_VERSION} · Chrome`,
      session: options.session ?? `lg_${Math.random().toString(36).slice(2, 12)}`,
      fingerprint: server.fingerprint,
      hint: options.hint ?? '',
    };
  };
  return [
    make('prod-web-01', 3 * 60e3, '连接成功', 'success', { duration: '1.18s' }),
    make('edge-cache-03', 18 * 60e3, '会话断开', 'success', {
      duration: '0.00s',
      source: sourceLabel.desktop,
    }),
    make('db-core-02', 42 * 60e3, '读取系统信息', 'success', { duration: '0.34s' }),
    make('build-runner-06', 72 * 60e3, '连接失败', 'failed', {
      duration: '2.41s',
      code: 'AUTH_FAILED',
      hint: '服务器拒绝身份认证。请确认用户名与密码或私钥是否匹配，并检查账号是否被锁定。',
    }),
    make('staging-app-04', 125 * 60e3, '连接失败', 'failed', {
      duration: '5.00s',
      source: sourceLabel.desktop,
      code: 'HOST_UNREACHABLE',
      hint: 'TCP 连接超时。请确认目标地址可达、安全组放行 22 端口，或 VPN / 跳板机状态正常。',
    }),
    make('prod-web-01', 201 * 60e3, '主机密钥已确认', 'success', { duration: '0.02s' }),
    make('backup-vault-05', 344 * 60e3, '连接成功', 'success', {
      duration: '1.42s',
      source: sourceLabel.desktop,
    }),
    make('edge-cache-03', 26 * 60e3 * 60 * 0.72, 'SSH 握手超时（已重试）', 'warning', {
      duration: '5.02s',
      code: 'CONNECT_TIMEOUT',
      hint: '首次握手在 5 秒内未完成，自动重试后成功。可能是瞬时网络抖动。',
    }),
    make('db-core-02', 26 * 60e3 * 60 * 0.82, '连接成功', 'success', { duration: '1.06s' }),
    make('prod-web-01', 26 * 60e3 * 60 * 0.91, '会话断开', 'success', {
      duration: '0.00s',
      source: sourceLabel.desktop,
    }),
    make('staging-app-04', 50 * 60e3 * 60, '连接成功', 'success', {
      duration: '1.61s',
      source: sourceLabel.desktop,
    }),
  ];
}
