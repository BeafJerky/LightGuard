import { getCurrentWindow } from '@tauri-apps/api/window';
import { onAction } from '@tauri-apps/plugin-notification';
import { navigateTo } from '../router';
import { isTauriRuntime } from '../ssh/tauri';
import { focusAlert } from './store';

// 通知里带的是告警 id（后端 `.extra("alertId", alert_id)`），点击后回到告警中心并定位该行。
function alertIdOf(extra: Record<string, unknown> | undefined): number {
  const raw = extra?.alertId;
  // 平台层可能把 extra 序列化成字符串，两种都收。
  return typeof raw === 'number' ? raw : Number(raw);
}

export async function startNotificationActions(): Promise<void> {
  // 浏览器里没有通知插件与窗口 API，注册会让开发服务器直接报错。
  if (!isTauriRuntime()) return;
  await onAction(async (event) => {
    // 只有点击通知本体（tap 或未上报 actionId）才跳转，按钮类操作不抢窗口。
    if (event.actionId && event.actionId !== 'tap') return;
    const alertId = alertIdOf(event.notification?.extra);
    // 恢复窗口是尽力而为：缺能力或平台不支持时不能拦住后面的跳转与定位。
    try {
      const appWindow = getCurrentWindow();
      // 顺序固定：先显示（托盘/隐藏态），再取消最小化，最后抢焦点。
      await appWindow.show();
      await appWindow.unminimize();
      await appWindow.setFocus();
    } catch (error) {
      console.error('[lightguard] notification tap could not restore the window', error);
    }
    navigateTo('alerts');
    // 0/NaN 只来自测试通知或旧版载荷，跳转到列表即可，不要定位到不存在的行。
    if (Number.isFinite(alertId) && alertId > 0) {
      await focusAlert(alertId);
    }
  });
}
