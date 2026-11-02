import { ref } from 'vue';

export const terminalTabCount = ref(0);

export function setTerminalTabCount(count: number) {
  terminalTabCount.value = Math.max(0, Math.floor(count));
}
