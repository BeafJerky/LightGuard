import type { AuditEntry } from '../ssh';

const OP_LABELS: Record<string, string> = {
  'fs.mkdir': '新建文件夹',
  'fs.mkdir.rollback': '删除文件夹（回退）',
  'docker.container.start': '启动容器',
  'docker.container.stop': '停止容器',
  'docker.container.restart': '重启容器',
  'docker.container.remove': '删除容器',
  'docker.container.start.rollback': '停止容器（回退）',
  'docker.container.stop.rollback': '启动容器（回退）',
  'docker.image.pull': '拉取镜像',
  'docker.image.remove': '删除镜像',
  'docker.service.start': '启动 Docker 服务',
  'docker.service.stop': '停止 Docker 服务',
  'docker.daemon.update': '更新 Docker 配置',
};

const ROLLBACKABLE = new Set(['fs.mkdir', 'docker.container.start', 'docker.container.stop']);

const RESULT_LABELS: Record<AuditEntry['result'], string> = {
  success: '成功',
  failed: '失败',
  unknown: '结果未知',
};

export function auditOpLabel(op: string): string {
  return OP_LABELS[op] ?? op;
}

export function auditParamsSummary(params: string): string {
  try {
    const value = JSON.parse(params) as Record<string, unknown>;
    if (typeof value.path === 'string') return value.path;
    if (typeof value.containerId === 'string') {
      const name = typeof value.name === 'string' && value.name ? value.name : '';
      const short = value.containerId.slice(0, 12);
      return name ? `${name}（${short}）` : short;
    }
    if (typeof value.image === 'string') return value.image;
    if (typeof value.file === 'string') return value.file;
    if (typeof value.service === 'string') {
      return `${value.service} · ${typeof value.action === 'string' ? value.action : '操作'}`;
    }
  } catch {
    /* params 不是 JSON 时原样展示 */
  }
  return params.slice(0, 80);
}

export function auditResultLabel(result: AuditEntry['result']): string {
  return RESULT_LABELS[result];
}

export function formatDurationMs(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(2)} s`;
}

export function canRollback(row: AuditEntry, rows: AuditEntry[]): boolean {
  return (
    ROLLBACKABLE.has(row.op) &&
    row.result === 'success' &&
    !rows.some((entry) => entry.rollbackOf === row.id)
  );
}
