import type { DockerContainer, DockerStatsSample } from './api';

export type ContainerLayout = 'card' | 'table';
export const LAYOUT_KEY = 'lg.docker.containers.layout';

function safeStorage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

export function normalizeLayout(value: string | null | undefined): ContainerLayout {
  return value === 'table' ? 'table' : 'card';
}

export function readLayout(): ContainerLayout {
  const storage = safeStorage();
  if (!storage) return 'card';
  try {
    return normalizeLayout(storage.getItem(LAYOUT_KEY));
  } catch {
    return 'card';
  }
}

export function writeLayout(layout: ContainerLayout): void {
  const storage = safeStorage();
  if (!storage) return;
  try {
    storage.setItem(LAYOUT_KEY, layout);
  } catch {
    // 隐私模式或配额不足时只保留本次会话的选择。
  }
}

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
  if (!term) return rows;
  // 只匹配名称、镜像、ID、端口与项目；compose 服务名包含在项目名中，不单独参与匹配。
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
