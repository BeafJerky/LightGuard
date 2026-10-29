//! Docker 应用与终端应用之间的容器终端请求队列。
//! 终端窗口按服务器挂载，因此用 serverId 过滤；同一容器重复请求只保留最新一条。

import { ref } from 'vue';

export interface ContainerTerminalRequest {
  serverId: string;
  containerId: string;
  name: string;
  shortId: string;
  title: string;
}

export const terminalRequests = ref<ContainerTerminalRequest[]>([]);

export function requestContainerTerminal(request: ContainerTerminalRequest): void {
  const rest = terminalRequests.value.filter(
    (item) => !(item.serverId === request.serverId && item.containerId === request.containerId),
  );
  terminalRequests.value = [...rest, request];
}

export function consumeContainerTerminal(serverId: string): ContainerTerminalRequest[] {
  const taken = terminalRequests.value.filter((item) => item.serverId === serverId);
  if (taken.length === 0) return [];
  terminalRequests.value = terminalRequests.value.filter((item) => item.serverId !== serverId);
  return taken;
}
