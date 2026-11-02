import { state, updateSettings } from '../store';
import { toast } from '../ui';

export interface AutostartBridge {
  isEnabled(): Promise<boolean>;
  enable(): Promise<void>;
  disable(): Promise<void>;
}

let bridgePromise: Promise<AutostartBridge> | null = null;

export function loadAutostartBridge(): Promise<AutostartBridge> {
  bridgePromise ??= import('@tauri-apps/plugin-autostart').then((module) => ({
    isEnabled: module.isEnabled,
    enable: module.enable,
    disable: module.disable,
  }));
  return bridgePromise;
}

export async function syncAutostart(bridge?: AutostartBridge): Promise<boolean> {
  try {
    const active = await (bridge ?? (await loadAutostartBridge())).isEnabled();
    if (active !== state.settings.autostart) updateSettings({ autostart: active });
    return active;
  } catch {
    toast({
      title: '无法读取开机自启状态',
      message: '系统未允许读取启动项，开关显示的状态可能与实际不一致。',
      tone: 'warning',
    });
    return state.settings.autostart;
  }
}

export async function setAutostart(
  value: boolean,
  bridge?: AutostartBridge,
): Promise<boolean> {
  try {
    const api = bridge ?? (await loadAutostartBridge());
    if (value) await api.enable();
    else await api.disable();
  } catch (error) {
    toast({
      title: '开机自启设置失败',
      message: error instanceof Error ? error.message : String(error),
      tone: 'error',
    });
    return false;
  }

  updateSettings({ autostart: value });
  toast({
    title: value ? '已开启开机自启' : '已关闭开机自启',
    message: value ? '登录 Windows 后将驻留托盘' : '下次登录不会再自动启动',
    tone: value ? 'success' : 'info',
  });
  return true;
}
