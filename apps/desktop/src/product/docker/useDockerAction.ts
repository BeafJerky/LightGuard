import { ref } from 'vue';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import {
  dockerApi,
  type DockerApi,
  type DockerPreview,
  type DockerSystemInput,
  type DockerWriteInput,
  type DockerWriteResult,
} from './api';

const MAX_AUTH_FAILURES = 3;

/**
 * Docker 级写操作（容器、镜像）：许可校验与写锁都在后端完成，
 * 前端只负责“取预览 → 确认 → 执行 → 展示输出”。
 */
export function useDockerWrite(server: Pick<Server, 'id'>, api: DockerApi = dockerApi) {
  const preview = ref<DockerPreview | null>(null);
  const busy = ref(false);
  const progress = ref<string[]>([]);
  const error = ref('');
  const done = ref<DockerWriteResult | null>(null);
  // 预览一旦交给后端执行就会被消费，避免用户再次点击同一个预览。
  const spent = ref(false);
  let lastInput: DockerWriteInput | null = null;

  function close(): void {
    preview.value = null;
    busy.value = false;
    progress.value = [];
    error.value = '';
    done.value = null;
    spent.value = false;
    lastInput = null;
  }

  async function ask(input: DockerWriteInput): Promise<boolean> {
    close();
    try {
      preview.value = await api.writePreview(server.id, input);
      lastInput = input;
      return true;
    } catch (caught) {
      error.value = toSshError(caught).message;
      return false;
    }
  }

  async function confirm(): Promise<DockerWriteResult | null> {
    const current = preview.value;
    if (!current || busy.value || spent.value) return null;
    busy.value = true;
    spent.value = true;
    error.value = '';
    try {
      done.value = await api.writeExecute(current.previewId, (line) => {
        progress.value = [...progress.value, line].slice(-8);
      });
      if (done.value.result === 'failed') error.value = done.value.message;
      return done.value;
    } catch (caught) {
      const failure = toSshError(caught);
      if (failure.code === 'PREVIEW_EXPIRED' && lastInput) {
        try {
          preview.value = await api.writePreview(server.id, lastInput);
          spent.value = false;
          progress.value = [];
          error.value = '预览已过期，已重新生成，请再次确认。';
        } catch (again) {
          error.value = toSshError(again).message;
        }
        return null;
      }
      error.value = failure.message;
      return null;
    } finally {
      busy.value = false;
    }
  }

  return { preview, busy, progress, error, done, spent, ask, confirm, close };
}

/**
 * 系统级操作（服务启停、daemon.json）：预览前先完成 sudo 授权。
 * 密码经 IPC 交给后端、由后端经 stdin 交给 sudo，前端不再持有副本。
 */
export function useDockerSystem(server: Pick<Server, 'id'>, api: DockerApi = dockerApi) {
  const preview = ref<DockerPreview | null>(null);
  const busy = ref(false);
  const progress = ref<string[]>([]);
  const error = ref('');
  // 终止态错误码：调用方据此决定是否给用户一条重试路径（如 DAEMON_INVALID）。
  const errorCode = ref('');
  const done = ref<DockerWriteResult | null>(null);
  const spent = ref(false);
  const authOpen = ref(false);
  const authMessage = ref('');
  const authError = ref('');
  const authBusy = ref(false);
  const failures = ref(0);
  let request: DockerSystemInput | null = null;

  function resetAuth(): void {
    authOpen.value = false;
    authMessage.value = '';
    authError.value = '';
    authBusy.value = false;
    failures.value = 0;
  }

  function close(): void {
    preview.value = null;
    busy.value = false;
    progress.value = [];
    error.value = '';
    errorCode.value = '';
    done.value = null;
    spent.value = false;
    request = null;
    resetAuth();
  }

  async function fetchPreview(password?: string, counted = false): Promise<boolean> {
    if (!request) return false;
    authBusy.value = true;
    try {
      preview.value = await api.systemPreview(server.id, request, password);
      errorCode.value = '';
      resetAuth();
      return true;
    } catch (caught) {
      const failure = toSshError(caught);
      errorCode.value = failure.code;
      if (failure.code === 'SUDO_REQUIRED') {
        authMessage.value = '该操作需要服务器账号的 sudo 权限。';
        authOpen.value = true;
        return false;
      }
      if (failure.code === 'SUDO_FAILED') {
        if (failure.message.includes('没有 sudo 权限')) {
          resetAuth();
          error.value = failure.message;
          return false;
        }
        if (counted) failures.value += 1;
        if (failures.value >= MAX_AUTH_FAILURES) {
          resetAuth();
          error.value = '连续 3 次授权失败：请检查 sudo 权限或改用有权限的账号。';
          return false;
        }
        authMessage.value = '系统授权失败，请重新输入 sudo 密码。';
        authError.value = failure.message;
        authOpen.value = true;
        return false;
      }
      resetAuth();
      error.value = failure.message;
      return false;
    } finally {
      authBusy.value = false;
    }
  }

  async function ask(input: DockerSystemInput): Promise<boolean> {
    close();
    request = input;
    return fetchPreview();
  }

  async function submit(password: string): Promise<boolean> {
    authError.value = '';
    if (!password) {
      authError.value = '请输入 sudo 密码';
      return false;
    }
    return fetchPreview(password, true);
  }

  async function confirm(): Promise<DockerWriteResult | null> {
    const current = preview.value;
    if (!current || busy.value || spent.value) return null;
    busy.value = true;
    spent.value = true;
    error.value = '';
    errorCode.value = '';
    try {
      done.value = await api.systemExecute(current.previewId, undefined, (line) => {
        progress.value = [...progress.value, line].slice(-8);
      });
      if (done.value.result === 'failed') error.value = done.value.message;
      return done.value;
    } catch (caught) {
      const failure = toSshError(caught);
      errorCode.value = failure.code;
      if (failure.code === 'SUDO_REQUIRED' || failure.code === 'SUDO_FAILED') {
        // 执行前授权失效：预览未被消费，重新走一次预览并等待密码。
        preview.value = null;
        spent.value = false;
        progress.value = [];
        done.value = null;
        await fetchPreview();
        return null;
      }
      error.value = failure.message;
      return null;
    } finally {
      busy.value = false;
    }
  }

  return {
    preview,
    busy,
    progress,
    error,
    errorCode,
    done,
    spent,
    authOpen,
    authMessage,
    authError,
    authBusy,
    failures,
    ask,
    submit,
    confirm,
    close,
  };
}
