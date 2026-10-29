import type { DockerImage, RegistrySearchItem } from './api';

export function filterImages(rows: DockerImage[], keyword: string): DockerImage[] {
  const term = keyword.trim().toLowerCase();
  if (!term) return rows;
  return rows.filter((row) =>
    [row.repo, row.shortId, row.id, ...row.tags].join(' ').toLowerCase().includes(term),
  );
}

/** 前三个标签直接展示，其余折叠成 `+N`；无标签视为悬空镜像。 */
export function tagSummary(tags: string[]): string {
  if (tags.length === 0) return '<none>';
  const head = tags.slice(0, 3).join('、');
  return tags.length > 3 ? `${head} +${tags.length - 3}` : head;
}

/** 删除镜像用：优先 `仓库:首个标签`，悬空或没有标签时退回完整镜像 ID。 */
export function imageReference(image: DockerImage): string {
  if (image.dangling || image.tags.length === 0 || !image.repo || image.repo === '<none>') {
    return image.id;
  }
  return `${image.repo}:${image.tags[0]}`;
}

/** 拉取引用：摘要形式原样使用；否则补齐标签（已有标签先剥离），缺省 latest。
 *  名称里已带标签且未另填（仍是默认 latest）时，保留名称自身的标签。 */
export function pullReference(name: string, tag: string): string {
  const repo = name.trim();
  if (repo.includes('@')) return repo;
  const slash = repo.lastIndexOf('/');
  const colon = repo.lastIndexOf(':');
  const base = colon > slash ? repo.slice(0, colon) : repo;
  const existing = colon > slash ? repo.slice(colon + 1) : '';
  const wanted = tag.trim() || 'latest';
  return `${base}:${wanted === 'latest' && existing ? existing : wanted}`;
}

export const QUICK_REGISTRY: RegistrySearchItem[] = [
  { name: 'nginx', description: '高性能 Web 服务器与反向代理', stars: 0, official: true, automated: false },
  { name: 'redis', description: '内存键值数据库与缓存', stars: 0, official: true, automated: false },
  { name: 'mysql', description: '关系型数据库', stars: 0, official: true, automated: false },
  { name: 'postgres', description: 'PostgreSQL 数据库', stars: 0, official: true, automated: false },
  { name: 'mongo', description: 'MongoDB 文档数据库', stars: 0, official: true, automated: false },
  { name: 'node', description: 'Node.js 运行时', stars: 0, official: true, automated: false },
  { name: 'python', description: 'Python 运行时', stars: 0, official: true, automated: false },
  { name: 'alpine', description: '轻量 Linux 基础镜像', stars: 0, official: true, automated: false },
  { name: 'ubuntu', description: 'Ubuntu 基础镜像', stars: 0, official: true, automated: false },
  { name: 'hello-world', description: 'Docker 安装验证镜像', stars: 0, official: true, automated: false },
];
