import { computed, ref } from 'vue';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { dockerApi, type DockerApi, type DockerCapabilities, type DockerOverview } from './api';

export function useDocker(server: Server, api: DockerApi = dockerApi) {
  const capabilities = ref<DockerCapabilities | null>(null);
  const overview = ref<DockerOverview | null>(null);
  const loading = ref(false);
  const error = ref('');
  const notice = ref('');

  const installed = computed(() => capabilities.value?.installed ?? false);
  const running = computed(() => capabilities.value?.daemonRunning ?? false);
  // 「服务未运行」与「没有 socket 权限」是两件事：后者只能用组权限解决，不能提示启动服务。
  const permissionDenied = computed(() => capabilities.value?.permissionDenied ?? false);

  async function load(includeDf = true, silent = false): Promise<void> {
    if (!silent) {
      loading.value = true;
      error.value = '';
    }
    try {
      capabilities.value = await api.capabilities(server.id);
      const usable = capabilities.value.installed && capabilities.value.daemonRunning;
      overview.value = usable ? await api.overview(server.id, includeDf) : null;
      error.value = '';
    } catch (caught) {
      if (!silent) {
        error.value = toSshError(caught).message;
        overview.value = null;
      }
    } finally {
      if (!silent) loading.value = false;
    }
  }

  return {
    api,
    capabilities,
    overview,
    loading,
    error,
    notice,
    installed,
    running,
    permissionDenied,
    load,
  };
}
