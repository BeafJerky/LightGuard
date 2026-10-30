import { describe, expect, it } from 'vitest';
import type { AuditEntry } from '../ssh';
import {
  auditOpLabel,
  auditParamsSummary,
  auditResultLabel,
  canRollback,
  formatDurationMs,
  rollbackLabel,
} from './audit';

function row(patch: Partial<AuditEntry> = {}): AuditEntry {
  return {
    id: 1,
    atMs: 1_700_000_000_000,
    serverId: 'srv_1',
    serverName: 'web-01',
    op: 'fs.mkdir',
    params: '{"path":"/vol1/1000/lg-filetest/demo"}',
    result: 'success',
    durationMs: 128,
    permitSource: 'unlock:15m',
    rollbackOf: null,
    ...patch,
  };
}

describe('audit helpers', () => {
  it('labels known operations and keeps unknown codes', () => {
    expect(auditOpLabel('fs.mkdir')).toBe('新建文件夹');
    expect(auditOpLabel('fs.mkdir.rollback')).toBe('删除文件夹（回退）');
    expect(auditOpLabel('fs.custom')).toBe('fs.custom');
  });

  it('extracts the path from params and falls back safely', () => {
    expect(auditParamsSummary(row().params)).toBe('/vol1/1000/lg-filetest/demo');
    expect(auditParamsSummary('not json')).toBe('not json');
    expect(auditParamsSummary('{"other":1}')).toBe('{"other":1}');
  });

  it('labels results and durations', () => {
    expect(auditResultLabel('success')).toBe('成功');
    expect(auditResultLabel('failed')).toBe('失败');
    expect(auditResultLabel('unknown')).toBe('结果未知');
    expect(formatDurationMs(128)).toBe('128 ms');
    expect(formatDurationMs(1_280)).toBe('1.28 s');
  });

  it('allows rollback only for a successful, not yet rolled back mkdir', () => {
    const original = row();
    expect(canRollback(original, [original])).toBe(true);
    expect(canRollback(row({ op: 'fs.mkdir.rollback', rollbackOf: 1 }), [])).toBe(false);
    expect(canRollback(row({ result: 'failed' }), [])).toBe(false);
    expect(canRollback(row({ result: 'unknown' }), [])).toBe(false);
    expect(canRollback(original, [original, row({ id: 2, op: 'fs.mkdir.rollback', rollbackOf: 1 })])).toBe(false);
    expect(
      canRollback(original, [original, row({ id: 2, op: 'fs.mkdir.rollback', result: 'failed', rollbackOf: 1 })]),
    ).toBe(true);
  });

  it('labels the rollback button by operation kind', () => {
    expect(rollbackLabel('docker.container.start')).toBe('回退（启动/停止互换）');
    expect(rollbackLabel('fs.mkdir')).toBe('回退（删除空目录）');
  });

  it('labels docker operations and their rollbacks', () => {
    expect(auditOpLabel('docker.container.start')).toBe('启动容器');
    expect(auditOpLabel('docker.container.stop')).toBe('停止容器');
    expect(auditOpLabel('docker.container.stop.rollback')).toBe('启动容器（回退）');
    expect(auditOpLabel('docker.image.pull')).toBe('拉取镜像');
    expect(auditOpLabel('docker.daemon.update')).toBe('更新 Docker 配置');
  });

  it('summarizes docker audit params without duplicating the short id', () => {
    const id = 'a'.repeat(64);
    expect(auditParamsSummary(`{"action":"stop","containerId":"${id}","name":"web（aaaaaaaaaaaa）"}`)).toBe(
      'web（aaaaaaaaaaaa）',
    );
    expect(auditParamsSummary(`{"action":"stop","containerId":"${id}","name":"web"}`)).toBe(
      'web（aaaaaaaaaaaa）',
    );
    expect(auditParamsSummary(`{"action":"stop","containerId":"${id}"}`)).toBe('aaaaaaaaaaaa');
    expect(auditParamsSummary('{"action":"pull","image":"nginx:1.27"}')).toBe('nginx:1.27');
    expect(auditParamsSummary('{"file":"/etc/docker/daemon.json","changed":["registry-mirrors"]}')).toBe(
      '/etc/docker/daemon.json',
    );
  });

  it('allows rollback only for container start and stop', () => {
    const start = row({ op: 'docker.container.start' });
    expect(canRollback(start, [start])).toBe(true);
    expect(canRollback(row({ op: 'docker.container.stop' }), [])).toBe(true);
    expect(canRollback(row({ op: 'docker.container.restart' }), [])).toBe(false);
    expect(canRollback(row({ op: 'docker.container.remove' }), [])).toBe(false);
    expect(
      canRollback(row({ op: 'docker.container.start', result: 'failed' }), []),
    ).toBe(false);
  });
});
