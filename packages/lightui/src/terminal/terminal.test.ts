import { describe, expect, it } from 'vitest';
import { mixColor, withAlpha } from '../render/paint/colors';
import { createSimulatedShell } from './simulator';
import { toXtermTheme } from './theme';
import { themes } from '../theme/tokens';

function collector() {
  let output = '';
  return { write: (text: string) => { output += text; }, text: () => output, clear: () => { output = ''; } };
}

describe('color helpers', () => {
  it('keeps existing alpha behavior for hex and rgba', () => {
    expect(withAlpha('#3ddc97', 0.5)).toBe('rgba(61, 220, 151, 0.5)');
    expect(withAlpha('rgba(1, 2, 3, 0.9)', 0.2)).toBe('rgba(1, 2, 3, 0.2)');
  });

  it('mixes toward a target color', () => {
    expect(mixColor('#000000', '#ffffff', 0.5)).toBe('#808080');
    expect(mixColor('#ff0000', '#000000', 1)).toBe('#000000');
    expect(mixColor('not-a-color', '#ffffff', 0.5)).toBe('not-a-color');
  });
});

describe('xterm theme', () => {
  it('maps every theme to a complete ANSI palette', () => {
    for (const theme of Object.values(themes)) {
      const mapped = toXtermTheme(theme);
      expect(mapped.background).toBe(theme.color.bg);
      expect(mapped.foreground).toBe(theme.color.text);
      expect(mapped.cursor).toBe(theme.color.primary);
      for (const key of ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'] as const) {
        expect(mapped[key]).toMatch(/^#[0-9a-f]{6}$/i);
      }
    }
  });

  it('uses a dark ANSI black in light themes', () => {
    const mapped = toXtermTheme(themes.light);
    expect(mapped.black).toBe(themes.light.color.textSecondary);
  });
});

describe('simulated shell', () => {
  it('prints a banner and prompt on start', () => {
    const sink = collector();
    const shell = createSimulatedShell({ user: 'ubuntu', hostname: 'web-prod-01', write: sink.write });
    shell.start();
    expect(sink.text()).toContain('LightGuard Shell');
    expect(sink.text()).toContain('ubuntu@web-prod-01');
    expect(sink.text()).toContain('$ ');
  });

  it('echoes input and executes whoami', () => {
    const sink = collector();
    const shell = createSimulatedShell({ user: 'ubuntu', hostname: 'web-prod-01', write: sink.write });
    shell.input('who');
    expect(shell.buffer()).toBe('who');
    shell.input('ami\r');
    expect(shell.buffer()).toBe('');
    expect(sink.text()).toContain('ubuntu\r\n');
    expect(sink.text()).toContain('ubuntu@web-prod-01');
  });

  it('handles backspace, ctrl+c and ctrl+l', () => {
    const sink = collector();
    const shell = createSimulatedShell({ user: 'ubuntu', hostname: 'web-prod-01', write: sink.write });
    shell.input('uptim');
    shell.input('\x7f');
    expect(shell.buffer()).toBe('upti');
    shell.input('me\r');
    expect(sink.text()).toContain('load average');

    sink.clear();
    shell.input('\x03');
    expect(sink.text()).toContain('^C');
    expect(shell.buffer()).toBe('');

    sink.clear();
    shell.input('\x0c');
    expect(sink.text()).toContain('\x1b[2J\x1b[H');
  });

  it('walks history with arrow keys', () => {
    const sink = collector();
    const shell = createSimulatedShell({ user: 'ubuntu', hostname: 'web-prod-01', write: sink.write });
    shell.input('ls\r');
    shell.input('whoami\r');
    shell.input('\x1b[A');
    expect(shell.buffer()).toBe('whoami');
    shell.input('\x1b[A');
    expect(shell.buffer()).toBe('ls');
    shell.input('\x1b[B');
    expect(shell.buffer()).toBe('whoami');
    shell.input('\x1b[B');
    expect(shell.buffer()).toBe('');
  });

  it('reports unknown commands', () => {
    const sink = collector();
    const shell = createSimulatedShell({ user: 'ubuntu', hostname: 'web-prod-01', write: sink.write });
    shell.input('nope\r');
    expect(sink.text()).toContain('bash: nope: command not found');
  });

  it('clears the screen on clear', () => {
    const sink = collector();
    const shell = createSimulatedShell({ user: 'ubuntu', hostname: 'web-prod-01', write: sink.write });
    shell.input('clear\r');
    expect(sink.text()).toContain('\x1b[2J\x1b[H');
  });
});
