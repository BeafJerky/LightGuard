import type { DockerContainer, DockerStatsSample } from './api';

export function mergeStats(
  rows: DockerContainer[],
  samples: DockerStatsSample[],
): DockerContainer[] {
  if (samples.length === 0) return rows;
  const byId = new Map(samples.map((sample) => [sample.id, sample]));
  return rows.map((row) => {
    const sample = byId.get(row.id);
    if (!sample) return row;
    return {
      ...row,
      cpuPercent: sample.cpuPercent,
      memUsage: sample.memUsage,
      memLimit: sample.memLimit,
      memPercent: sample.memPercent,
      netRx: sample.netRx,
      netTx: sample.netTx,
    };
  });
}

export function filterContainers(rows: DockerContainer[], keyword: string): DockerContainer[] {
  const term = keyword.trim().toLowerCase();
  // 关键词为空时直接返回原数组（同一引用，不复制）。
  if (!term) return rows;
  // 列表搜索名称、镜像、ID、端口与项目；compose 服务名不参与搜索，
  // 因为规范把筛选限定为 名称/镜像/项目，且 compose 容器名通常已包含服务名。
  return rows.filter((row) =>
    [row.name, row.image, row.id, row.ports, row.project]
      .join(' ')
      .toLowerCase()
      .includes(term),
  );
}

const PRIORITY: Record<string, number> = {
  running: 0,
  restarting: 1,
  paused: 2,
  created: 3,
  exited: 4,
  dead: 5,
};

export function sortContainers(rows: DockerContainer[]): DockerContainer[] {
  return [...rows].sort((left, right) => {
    const rank = (PRIORITY[left.state] ?? 9) - (PRIORITY[right.state] ?? 9);
    if (rank !== 0) return rank;
    return left.name.localeCompare(right.name);
  });
}
