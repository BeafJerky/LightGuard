import { describe, expect, it } from 'vitest';
import { containerActions, remainingSeconds } from './actions';

describe('docker container actions', () => {
  it('offers restart and stop for active containers', () => {
    expect(containerActions('running').map((option) => option.kind)).toEqual([
      'container.restart',
      'container.stop',
    ]);
    expect(containerActions('paused').map((option) => option.kind)).toContain('container.stop');
  });

  it('offers start and remove for stopped containers', () => {
    const options = containerActions('exited');
    expect(options.map((option) => option.kind)).toEqual(['container.start', 'container.remove']);
    expect(options[1].danger).toBe(true);
  });

  it('hides actions while the container is being removed', () => {
    expect(containerActions('removing')).toEqual([]);
  });

  it('counts down preview expiry in whole seconds', () => {
    expect(remainingSeconds(61_000, 1_000)).toBe(60);
    expect(remainingSeconds(1_000, 999)).toBe(1);
    expect(remainingSeconds(0, 5_000)).toBe(0);
    expect(remainingSeconds(Number.NaN, 0)).toBe(0);
  });
});
