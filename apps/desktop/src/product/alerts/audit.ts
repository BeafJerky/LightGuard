import type { AuditEntry } from '../ssh';

const OP_LABELS: Record<string, string> = {
  'fs.mkdir': '新建文件夹',
  'fs.mkdir.rollback': '删除文件夹（回退）',
  'fs.rename': '重命名',
  'fs.rename.rollback': '重命名（回退）',
  'fs.chmod': '修改权限',
  'fs.chmod.rollback': '恢复权限（回退）',
  'fs.delete': '删除',
  'fs.upload': '上传文件',
  'fs.upload.rollback': '删除上传文件（回退）',
  'docker.container.start': '启动容器',
  'docker.container.stop': '停止容器',
  'docker.container.restart': '重启容器',
  'docker.container.remove': '删除容器',
  'docker.container.start.rollback': '停止容器（回退）',
  'docker.container.stop.rollback': '启动容器（回退）',
  'docker.image.pull': '拉取镜像',
  'docker.image.remove': '删除镜像',
  'docker.image.push': '推送镜像',
  'docker.image.prune': '清理悬空镜像',
  'docker.image.import': '导入镜像',
  'docker.image.build': '构建镜像',
  'docker.registry.login': '登录镜像仓库',
  'docker.registry.logout': '退出镜像仓库',
  'docker.service.start': '启动 Docker 服务',
  'docker.service.stop': '停止 Docker 服务',
  'docker.daemon.update': '更新 Docker 配置',
  'docker.compose.start': '启动 Compose 项目',
  'docker.compose.stop': '停止 Compose 项目',
  'docker.compose.start.rollback': '停止 Compose 项目（回退）',
  'docker.compose.stop.rollback': '启动 Compose 项目（回退）',
  'docker.network.create': '创建网络',
  'docker.network.create.rollback': '删除网络（回退）',
  'docker.network.remove': '删除网络',
  'docker.network.prune': '清理未使用网络',
  'docker.volume.create': '创建数据卷',
  'docker.volume.create.rollback': '删除数据卷（回退）',
  'docker.volume.remove': '删除数据卷',
  'docker.volume.prune': '清理未使用数据卷',
};

const ROLLBACKABLE = new Set([
  'fs.mkdir',
  'fs.rename',
  'fs.chmod',
  'fs.upload',
  'docker.container.start',
  'docker.container.stop',
  'docker.compose.start',
  'docker.compose.stop',
  'docker.network.create',
  'docker.volume.create',
]);

const RESULT_LABELS: Record<AuditEntry['result'], string> = {
  success: '成功',
  failed: '失败',
  unknown: '结果未知',
};

export function auditOpLabel(op: string): string {
  return OP_LABELS[op] ?? op;
}

const SHORT_SUFFIX = /（[0-9a-f]{12}）$/;

export function auditParamsSummary(params: string): string {
  try {
    const value = JSON.parse(params) as Record<string, unknown>;
    if (typeof value.path === 'string') return value.path;
    if (typeof value.from === 'string' && typeof value.to === 'string') {
      return `${value.from} → ${value.to}`;
    }
    if (typeof value.containerId === 'string') {
      const id = value.containerId;
      const short = id.slice(0, 12);
      const raw = typeof value.name === 'string' ? value.name : '';
      // 真实审计行里的 name 已是「名（短ID）」；旧行可能是裸名。
      const name = raw.replace(SHORT_SUFFIX, '');
      return name ? `${name}（${short}）` : short;
    }
    if (typeof value.image === 'string') return value.image;
    if (typeof value.target === 'string') return value.target;
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
  if (row.op === 'fs.upload') {
    try {
      const params = JSON.parse(row.params) as { overwrite?: boolean };
      if (params.overwrite) return false;
    } catch {
      return false;
    }
  }
  return (
    ROLLBACKABLE.has(row.op) &&
    row.result === 'success' &&
    !rows.some((entry) => entry.rollbackOf === row.id && entry.result === 'success')
  );
}

/** 审计回退按钮的文案：容器启停与实际删除空目录语义不同。 */
export function rollbackLabel(op: string): string {
  if (op === 'fs.rename') return '回退（改回原名称）';
  if (op === 'fs.chmod') return '回退（恢复原权限）';
  if (op.startsWith('docker.compose.') || op.startsWith('docker.container.')) {
    return '回退（启动/停止互换）';
  }
  if (op.startsWith('docker.network.') || op.startsWith('docker.volume.')) {
    return '回退（删除创建的对象）';
  }
  return '回退（删除空目录）';
}
