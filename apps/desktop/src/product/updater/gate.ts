import { guardState } from '../guard/store';
import { state } from '../store';
import { terminalTabCount } from '../terminal/activity';

export interface UpdateGate {
  connections: number;
  terminals: number;
  pendingWrites: number;
  total: number;
}

export function updateGate(): UpdateGate {
  const connections = state.servers.filter(
    (server) =>
      server.status === 'online' ||
      server.status === 'warning' ||
      server.status === 'connecting',
  ).length;
  const terminals = terminalTabCount.value;
  const pendingWrites = guardState.preview ? 1 : 0;
  return {
    connections,
    terminals,
    pendingWrites,
    total: connections + terminals + pendingWrites,
  };
}

export function gateMessage(gate: UpdateGate): string | null {
  if (gate.total === 0) return null;
  const parts: string[] = [];
  if (gate.connections) parts.push(`${gate.connections} 个服务器连接`);
  if (gate.terminals) parts.push(`${gate.terminals} 个终端标签`);
  if (gate.pendingWrites) parts.push('1 个待确认的写操作');
  return `更新将断开 ${parts.join('、')}并结束未完成的写操作确认，是否继续？`;
}
