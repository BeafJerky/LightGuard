import { AUTH_LABELS, HISTORY_POINTS, round } from '../data';
import type {
  PersistedServer,
  Server,
  ServerMetrics,
  ServerSample,
  ServerStatus,
  ServerDraft,
} from '../types';
import type { BackendAuth, ConnectResult, ServerSnapshot } from './backend';

export interface ConnectMeta {
  serverId: string;
  draft: ServerDraft;
  auth: BackendAuth;
}

function deviceType(kind: string) {
  return kind || '—';
}

function metricsOf(snapshot: ServerSnapshot): ServerMetrics {
  return {
    cpu: Math.round(snapshot.cpuPercent),
    mem: Math.round(snapshot.memPercent),
    disk: Math.round(snapshot.diskPercent),
    down: round(snapshot.downMbps, 1),
    up: round(snapshot.upMbps, 1),
  };
}

export function formatUptime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return '—';
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分钟`;
  if (minutes > 0) return `${minutes} 分钟`;
  return `${Math.floor(seconds)} 秒`;
}

export function warningText(snapshot: ServerSnapshot): string {
  const issues: string[] = [];
  if (snapshot.cpuPercent >= 93) issues.push(`CPU ${Math.round(snapshot.cpuPercent)}%`);
  if (snapshot.memPercent >= 90) issues.push(`内存 ${Math.round(snapshot.memPercent)}%`);
  if (snapshot.diskPercent >= 90) issues.push(`磁盘 ${Math.round(snapshot.diskPercent)}%`);
  return issues.length ? `资源使用率偏高：${issues.join(' · ')}。` : '';
}

export function noticeText(snapshot: ServerSnapshot): string {
  if (snapshot.diskPercent >= 85 && snapshot.diskPercent < 90) {
    return `磁盘使用率 ${Math.round(snapshot.diskPercent)}%，建议清理或扩容。`;
  }
  return '';
}

function statusOf(snapshot: ServerSnapshot): ServerStatus {
  return warningText(snapshot) ? 'warning' : 'online';
}

function sampleOf(snapshot: ServerSnapshot): ServerSample {
  return {
    ts: Date.parse(snapshot.sampledAt) || Date.now(),
    cpu: Math.round(snapshot.cpuPercent),
    mem: Math.round(snapshot.memPercent),
    down: round(snapshot.downMbps, 1),
    up: round(snapshot.upMbps, 1),
    processes: snapshot.processCount,
  };
}

function seedSample(snapshot: ServerSnapshot, metrics: ServerMetrics): ServerSample[] {
  const parsed = Date.parse(snapshot.sampledAt);
  if (Number.isFinite(parsed)) return [sampleOf(snapshot)];
  return [
    {
      ts: Date.now(),
      cpu: metrics.cpu,
      mem: metrics.mem,
      down: metrics.down,
      up: metrics.up,
      processes: snapshot.processCount,
    },
  ];
}

function pushSample(samples: ServerSample[], sample: ServerSample): ServerSample[] {
  const next = samples.slice(-(HISTORY_POINTS - 1));
  next.push(sample);
  return next;
}

function pushHistory(history: number[], value: number): number[] {
  const next = history.length >= HISTORY_POINTS ? history.slice(-(HISTORY_POINTS - 1)) : history.slice();
  next.push(round(value, 1));
  return next;
}

export function newServer(meta: ConnectMeta, result: ConnectResult): Server {
  const { draft } = meta;
  const snapshot = result.snapshot;
  const metrics = metricsOf(snapshot);
  const authLabel = AUTH_LABELS[meta.auth];

  return {
    id: meta.serverId,
    name: draft.name.trim() || snapshot.hostname || draft.ip,
    hostname: snapshot.hostname,
    role: draft.role.trim() || '通用服务器',
    ip: draft.ip.trim(),
    port: draft.port,
    region: 'SSH 直连',
    group: draft.group.trim() || '未分组',
    tags: draft.tags.length ? draft.tags : ['自建'],
    note: draft.note.trim(),
    status: statusOf(snapshot),
    os: snapshot.os,
    kernel: snapshot.kernel,
    arch: snapshot.arch,
    cpuModel: snapshot.cpuModel,
    cpuCores: snapshot.cpuCores,
    cpuFreq: snapshot.cpuFreq,
    memTotal: round(snapshot.memTotalGb, 1),
    memType: '—',
    diskTotal: round(snapshot.diskTotalGb, 1),
    user: draft.user.trim(),
    auth: meta.auth,
    authLabel,
    fingerprint: result.fingerprint,
    uptime: formatUptime(snapshot.uptimeSeconds),
    bootTime: snapshot.bootTime,
    timezone: snapshot.timezone,
    lastSync: '刚刚',
    lastSeen: '刚刚',
    latency: result.latencyMs,
    load: snapshot.load,
    temp: Math.round(snapshot.tempC),
    warning: warningText(snapshot),
    notice: noticeText(snapshot),
    error: null,
    needsAuth: false,
    metrics,
    history: {
      cpu: pushHistory([], metrics.cpu),
      mem: pushHistory([], metrics.mem),
      net: pushHistory([], metrics.down),
    },
    cpuCoresLoad: snapshot.cpuCoresLoad.map((value) => round(value, 1)),
    memAvailable: round(snapshot.memAvailableGb, 1),
    swapTotal: round(snapshot.swapTotalGb, 1),
    swapUsed: round(snapshot.swapUsedGb, 1),
    disks: snapshot.disks.map((disk) => ({
      mount: disk.mount,
      fs: disk.fs,
      type: deviceType(disk.kind),
      total: round(disk.totalGb, 1),
      used: round(disk.usedGb, 1),
      inodePercent: round(disk.inodePercent, 1),
    })),
    interfaces: snapshot.interfaces.map((iface) => ({
      name: iface.name,
      ip: iface.ip,
      mac: iface.mac,
      speed: iface.speed,
      down: round(iface.downMbps, 1),
      up: round(iface.upMbps, 1),
      rxTotal: round(iface.rxTotalGb, 2),
      txTotal: round(iface.txTotalGb, 2),
    })),
    gateway: snapshot.gateway,
    dns: snapshot.dns,
    publicIp: snapshot.publicIp,
    mtu: snapshot.mtu,
    processes: snapshot.processes.map((process) => ({
      pid: process.pid,
      name: process.name,
      user: process.user,
      cpu: round(process.cpu, 1),
      mem: round(process.mem, 1),
      state: process.state,
    })),
    processCount: snapshot.processCount,
    sampledAt: snapshot.sampledAt,
    sessionId: result.sessionId,
    startedAt: result.startedAt,
    hostKeyState: result.hostKeyState,
    encryption: result.encryption ?? null,
    samples: seedSample(snapshot, metrics),
  };
}

export function applySnapshot(server: Server, snapshot: ServerSnapshot): Server {
  const metrics = metricsOf(snapshot);
  const sample = sampleOf(snapshot);
  return Object.assign(server, {
    hostname: snapshot.hostname,
    status: statusOf(snapshot),
    os: snapshot.os,
    kernel: snapshot.kernel,
    arch: snapshot.arch,
    cpuModel: snapshot.cpuModel,
    cpuCores: snapshot.cpuCores,
    cpuFreq: snapshot.cpuFreq,
    memTotal: round(snapshot.memTotalGb, 1),
    diskTotal: round(snapshot.diskTotalGb, 1),
    uptime: formatUptime(snapshot.uptimeSeconds),
    bootTime: snapshot.bootTime,
    timezone: snapshot.timezone,
    lastSync: '刚刚',
    lastSeen: '刚刚',
    load: snapshot.load,
    temp: Math.round(snapshot.tempC),
    warning: warningText(snapshot),
    notice: noticeText(snapshot),
    error: null,
    needsAuth: false,
    metrics,
    history: {
      cpu: pushHistory(server.history.cpu, metrics.cpu),
      mem: pushHistory(server.history.mem, metrics.mem),
      net: pushHistory(server.history.net, metrics.down),
    },
    cpuCoresLoad: snapshot.cpuCoresLoad.map((value) => round(value, 1)),
    memAvailable: round(snapshot.memAvailableGb, 1),
    swapTotal: round(snapshot.swapTotalGb, 1),
    swapUsed: round(snapshot.swapUsedGb, 1),
    disks: snapshot.disks.map((disk) => ({
      mount: disk.mount,
      fs: disk.fs,
      type: deviceType(disk.kind),
      total: round(disk.totalGb, 1),
      used: round(disk.usedGb, 1),
      inodePercent: round(disk.inodePercent, 1),
    })),
    interfaces: snapshot.interfaces.map((iface) => ({
      name: iface.name,
      ip: iface.ip,
      mac: iface.mac,
      speed: iface.speed,
      down: round(iface.downMbps, 1),
      up: round(iface.upMbps, 1),
      rxTotal: round(iface.rxTotalGb, 2),
      txTotal: round(iface.txTotalGb, 2),
    })),
    gateway: snapshot.gateway,
    dns: snapshot.dns,
    publicIp: snapshot.publicIp,
    mtu: snapshot.mtu,
    processes: snapshot.processes.map((process) => ({
      pid: process.pid,
      name: process.name,
      user: process.user,
      cpu: round(process.cpu, 1),
      mem: round(process.mem, 1),
      state: process.state,
    })),
    processCount: snapshot.processCount,
    sampledAt: snapshot.sampledAt,
    samples: pushSample(server.samples, sample),
  });
}

export function markConnected(server: Server, result: ConnectResult) {
  server.sessionId = result.sessionId;
  server.startedAt = result.startedAt;
  server.fingerprint = result.fingerprint;
  server.hostKeyState = result.hostKeyState;
  server.encryption = result.encryption ?? null;
  server.latency = result.latencyMs;
  server.authLabel = result.authLabel;
  server.lastSeen = '刚刚';
}

export function toPersisted(server: Server): PersistedServer {
  return {
    id: server.id,
    name: server.name,
    hostname: server.hostname,
    role: server.role,
    ip: server.ip,
    port: server.port,
    region: server.region,
    group: server.group,
    tags: server.tags.slice(),
    note: server.note,
    user: server.user,
    auth: server.auth,
    authLabel: server.authLabel,
    fingerprint: server.fingerprint,
    lastSync: server.lastSync,
    lastSeen: server.lastSeen,
    cpuCores: server.cpuCores,
    memTotal: server.memTotal,
    diskTotal: server.diskTotal,
    os: server.os,
    kernel: server.kernel,
    arch: server.arch,
    cpuModel: server.cpuModel,
    cpuFreq: server.cpuFreq,
    timezone: server.timezone,
  };
}

export function fromPersisted(saved: PersistedServer): Server {
  const emptyMetrics = { cpu: 0, mem: 0, disk: 0, down: 0, up: 0 };
  return {
    id: saved.id,
    name: saved.name,
    hostname: saved.hostname || saved.name,
    role: saved.role,
    ip: saved.ip,
    port: saved.port,
    region: saved.region,
    group: saved.group,
    tags: Array.isArray(saved.tags) ? saved.tags.slice() : [],
    note: saved.note ?? '',
    status: 'offline',
    os: saved.os || '待读取',
    kernel: saved.kernel || '—',
    arch: saved.arch || '—',
    cpuModel: saved.cpuModel || '—',
    cpuCores: saved.cpuCores || 0,
    cpuFreq: saved.cpuFreq || '—',
    memTotal: saved.memTotal || 0,
    memType: '—',
    diskTotal: saved.diskTotal || 0,
    user: saved.user,
    auth: saved.auth,
    authLabel: saved.authLabel || AUTH_LABELS[saved.auth] || '—',
    fingerprint: saved.fingerprint || '—',
    uptime: '—',
    bootTime: '—',
    timezone: saved.timezone || '—',
    lastSync: saved.lastSync || '尚未连接',
    lastSeen: saved.lastSeen || '尚未连接',
    latency: 0,
    load: [0, 0, 0],
    temp: 0,
    warning: '',
    notice: '',
    error: null,
    needsAuth: false,
    metrics: { ...emptyMetrics },
    history: { cpu: [], mem: [], net: [] },
    cpuCoresLoad: [],
    memAvailable: 0,
    swapTotal: 0,
    swapUsed: 0,
    disks: [],
    interfaces: [],
    gateway: '—',
    dns: '—',
    publicIp: '—',
    mtu: '—',
    processes: [],
    processCount: 0,
    sampledAt: '',
    sessionId: '',
    startedAt: '',
    hostKeyState: '',
    encryption: null,
    samples: [],
  };
}
