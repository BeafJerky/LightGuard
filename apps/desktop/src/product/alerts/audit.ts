import type { AuditEntry } from '../ssh';

const OP_LABELS: Record<string, string> = {
  'fs.mkdir': '新建文件夹',
  'fs.mkdir.rollback': '删除文件夹（回退）',
};

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
    row.op === 'fs.mkdir' &&
    row.result === 'success' &&
    !rows.some((entry) => entry.rollbackOf === row.id)
  );
}
