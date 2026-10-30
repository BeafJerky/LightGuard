import { beforeEach, describe, expect, it } from 'vitest';
import { consumeContainerTerminal, requestContainerTerminal, terminalRequests } from './bus';

const base = {
  serverId: 'srv',
  containerId: 'a'.repeat(64),
  name: 'web',
  shortId: 'a'.repeat(12),
  title: 'web · nas',
};

beforeEach(() => {
  terminalRequests.value = [];
});

describe('container terminal bus', () => {
  it('同一容器只保留最新一条请求', () => {
    requestContainerTerminal(base);
    requestContainerTerminal({ ...base, name: 'web2', title: 'web2 · nas' });
    expect(terminalRequests.value).toHaveLength(1);
    expect(terminalRequests.value[0].name).toBe('web2');
  });

  it('只消费目标服务器的请求并清空该服务器队列', () => {
    requestContainerTerminal(base);
    requestContainerTerminal({ ...base, serverId: 'other', containerId: 'b'.repeat(64) });
    const taken = consumeContainerTerminal('srv');
    expect(taken).toHaveLength(1);
    expect(terminalRequests.value.map((item) => item.serverId)).toEqual(['other']);
  });

  it('队列为空时返回空数组', () => {
    expect(consumeContainerTerminal('srv')).toEqual([]);
  });
});
