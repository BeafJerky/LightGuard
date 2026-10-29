//! Docker 权限空态的统一文案：socket 受限时提示加入 docker 组，
//! 并明确 LightGuard 不会自动提权（实机验收 0.13.0 发现缺失）。

export const PERMISSION_HINT =
  '请将账号加入 docker 组，或调整 /var/run/docker.sock 权限；LightGuard 不会自动提权。';

/** 只有 socket 权限不足才追加组权限提示，其它错误保持原样。 */
export function permissionHint(code: string): string {
  return code === 'DOCKER_PERMISSION' ? PERMISSION_HINT : '';
}
