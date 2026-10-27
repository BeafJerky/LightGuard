<script setup lang="ts">
import { ref } from 'vue';
import { LgButton, LgIcon, LgTerminal } from '@lightguard/lightui';
import type { Server } from '../types';

const props = defineProps<{
  server: Server;
  height: number;
}>();

const terminalRef = ref<{ focus: () => void; write: (text: string) => void } | null>(null);

function clearScreen() {
  terminalRef.value?.write('\u001b[2J\u001b[H');
  terminalRef.value?.focus();
}
</script>

<template>
  <div class="lgapp-terminal">
    <div class="lgapp-toolbar">
      <span class="lgapp-terminal__note">
        <LgIcon name="terminal" :size="13" />
        {{ server.user }}@{{ server.name }} · 本地模拟 Shell
      </span>
      <div class="lgapp-actions">
        <LgButton size="tiny" secondary @click="terminalRef?.focus()">
          <template #icon><LgIcon name="play" :size="12" /></template>
          聚焦
        </LgButton>
        <LgButton size="tiny" secondary @click="clearScreen">
          <template #icon><LgIcon name="trash" :size="12" /></template>
          清屏
        </LgButton>
      </div>
    </div>

    <LgTerminal
      ref="terminalRef"
      :height="height"
      :label="`${server.name} · 模拟 Shell`"
      :user="server.user"
      :hostname="server.name"
    />

    <p class="lgapp-terminal__note">
      <LgIcon name="info" :size="12" />
      输入 help 查看可用命令。原型不会发起真实 SSH 连接，切换渲染后端时会保留当前会话。
    </p>
  </div>
</template>

<style scoped>
.lgapp-terminal .lgapp-actions {
  margin-left: auto;
}
</style>
