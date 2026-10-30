//! Docker 权限空态的统一文案：socket 受限时提示加入 docker 组，
//! 并明确 LightGuard 不会自动提权（实机验收 0.13.0 发现缺失）。

export const PERMISSION_HINT =
  '请将账号加入 docker 组，或调整 /var/run/docker.sock 权限；LightGuard 不会自动提权。';

/** 只有 socket 权限不足才追加组权限提示，其它错误保持原样。 */
export function permissionHint(code: string): string {
  return code === 'DOCKER_PERMISSION' ? PERMISSION_HINT : '';
}

/** 搜索由服务器直连 Docker Hub，加速源只作用于拉取，失败时给出可操作的下一步。 */
export const REGISTRY_HINT =
  '加速源只作用于拉取；搜索需要服务器能访问 Docker Hub，失败时可重试或直接拉取。';

export function registryHint(code: string): string {
  return code === 'REGISTRY_FAILED' ? REGISTRY_HINT : '';
}

/** 查版本只读加速源的 tags/list，不经过 Docker Hub，失败提示不能套用搜索文案。 */
export const TAG_HINT =
  '查版本读取的是加速源上的标签列表，不经过 Docker Hub；加速源不可达时请检查「加速源设置」。';

export function tagHint(code: string): string {
  return code === 'REGISTRY_FAILED' ? TAG_HINT : '';
}
