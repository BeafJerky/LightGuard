import { beforeEach, describe, expect, it } from 'vitest';
import { state } from '../store';
import { guardState } from '../guard/store';
import { setTerminalTabCount } from '../terminal/activity';
import { gateMessage, updateGate } from './gate';
import type { Server } from '../types';
import type { WritePreview } from '../ssh';

beforeEach(() => {
  state.servers.splice(0, state.servers.length);
  setTerminalTabCount(0);
  guardState.preview = null;
});

describe('update install gate', () => {
  it('is empty when nothing is active', () => {
    expect(updateGate().total).toBe(0);
    expect(gateMessage(updateGate())).toBeNull();
  });

  it('counts online, warning and connecting servers', () => {
    state.servers.push(
      { id: 's1', status: 'online' } as Server,
      { id: 's2', status: 'warning' } as Server,
      { id: 's3', status: 'connecting' } as Server,
      { id: 's4', status: 'offline' } as Server,
    );
    expect(updateGate().connections).toBe(3);
  });

  it('counts terminal tabs and a pending write preview', () => {
    setTerminalTabCount(2);
    // gate 只关心「是否存在待确认的写操作」，不需要构造完整的 WritePreview。
    guardState.preview = { serverId: 's1' } as unknown as WritePreview;
    const gate = updateGate();
    expect(gate.terminals).toBe(2);
    expect(gate.pendingWrites).toBe(1);
    expect(gateMessage(gate)).toContain('2 个终端标签');
    expect(gateMessage(gate)).toContain('待确认的写操作');
  });
});
