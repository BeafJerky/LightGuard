<script setup lang="ts">
import ConnectionApp from './ConnectionApp.vue';
import DisksApp from './DisksApp.vue';
import NetworkApp from './NetworkApp.vue';
import PerformanceApp from './PerformanceApp.vue';
import ProcessesApp from './ProcessesApp.vue';
import SystemInfoApp from './SystemInfoApp.vue';
import TerminalApp from './TerminalApp.vue';
import type { DesktopAppKey } from './apps';
import type { Server } from '../types';

defineProps<{
  app: DesktopAppKey;
  server: Server;
  session: string;
  startedAt: string;
}>();

const emit = defineEmits<{ disconnect: [] }>();
</script>

<template>
  <SystemInfoApp v-if="app === 'about'" :server="server" />
  <PerformanceApp v-else-if="app === 'performance'" :server="server" />
  <DisksApp v-else-if="app === 'disks'" :server="server" />
  <NetworkApp v-else-if="app === 'network'" :server="server" />
  <ProcessesApp v-else-if="app === 'processes'" :server="server" />
  <ConnectionApp
    v-else-if="app === 'connection'"
    :server="server"
    :session="session"
    :started-at="startedAt"
    @disconnect="emit('disconnect')"
  />
  <TerminalApp v-else :server="server" />
</template>
