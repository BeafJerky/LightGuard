const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';
const DIM = '\x1b[2m';
const RED = '\x1b[31m';
const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const BLUE = '\x1b[34m';
const CYAN = '\x1b[36m';

export interface SimulatedShellOptions {
  user: string;
  hostname: string;
  now?: () => Date;
  write: (text: string) => void;
}

export interface SimulatedShell {
  start: () => void;
  input: (data: string) => void;
  buffer: () => string;
}

const HELP_LINES = [
  ['help', '显示此帮助'],
  ['uptime', '系统运行时间与负载'],
  ['ls', '列出当前目录'],
  ['whoami', '当前登录用户'],
  ['date', '当前时间'],
  ['echo …', '回显参数'],
  ['clear', '清屏（Ctrl+L）'],
  ['exit', '结束模拟会话'],
];

export function createSimulatedShell(options: SimulatedShellOptions): SimulatedShell {
  const { user, hostname, write } = options;
  const now = options.now ?? (() => new Date());
  const history: string[] = [];
  let historyIndex = 0;
  let line = '';

  const prompt = () => `${GREEN}${user}@${hostname}${RESET}:${BLUE}~${RESET}$ `;

  const replaceLine = (text: string) => {
    line = text;
    write(`\r\x1b[K${prompt()}${line}`);
  };

  const command = (input: string) => {
    const [name, ...args] = input.trim().split(/\s+/);
    switch (name) {
      case 'help':
        write(`${BOLD}可用命令${RESET}\r\n`);
        HELP_LINES.forEach(([cmd, desc]) => {
          write(`  ${CYAN}${cmd.padEnd(10, ' ')}${RESET}${DIM}${desc}${RESET}\r\n`);
        });
        write(`${DIM}↑/↓ 浏览历史，Ctrl+C 中断当前行。${RESET}\r\n`);
        return;
      case 'uptime':
        write(` ${now().toLocaleTimeString('zh-CN', { hour12: false })} up 12 days,  4:31,  1 user,  load average: 0.42, 0.38, 0.31\r\n`);
        return;
      case 'ls':
        write(`${BLUE}lightguard-agent/${RESET}  ${BLUE}logs/${RESET}  ${BLUE}etc/${RESET}  README.md\r\n`);
        return;
      case 'whoami':
        write(`${user}\r\n`);
        return;
      case 'date':
        write(`${now().toString()}\r\n`);
        return;
      case 'echo':
        write(`${args.join(' ')}\r\n`);
        return;
      case 'clear':
        write('\x1b[2J\x1b[H');
        return;
      case 'exit':
        write(`${YELLOW}模拟会话不会退出；接入真实 SSH 后此处会关闭会话。${RESET}\r\n`);
        return;
      default:
        write(`${RED}bash: ${name}: command not found${RESET}\r\n`);
    }
  };

  const submit = () => {
    write('\r\n');
    const input = line;
    line = '';
    if (!input.trim()) return;
    if (history[history.length - 1] !== input) history.push(input);
    historyIndex = history.length;
    command(input);
  };

  const input = (data: string) => {
    for (let index = 0; index < data.length; index += 1) {
      const char = data[index];

      if (char === '\x1b') {
        const sequence = data.slice(index, index + 3);
        if (sequence === '\x1b[A' || sequence === '\x1b[B') {
          index += 2;
          if (!history.length) continue;
          historyIndex = sequence === '\x1b[A'
            ? Math.max(0, historyIndex - 1)
            : Math.min(history.length, historyIndex + 1);
          replaceLine(history[historyIndex] ?? '');
        }
        continue;
      }

      if (char === '\r' || char === '\n') {
        if (char === '\n' && data[index - 1] === '\r') continue;
        submit();
        write(prompt());
        continue;
      }
      if (char === '\x7f') {
        if (line.length) {
          line = line.slice(0, -1);
          write('\b \b');
        }
        continue;
      }
      if (char === '\x03') {
        write(`^C\r\n${prompt()}`);
        line = '';
        continue;
      }
      if (char === '\x0c') {
        write('\x1b[2J\x1b[H');
        write(`${prompt()}${line}`);
        continue;
      }
      if (char === '\x15') {
        replaceLine('');
        continue;
      }
      if (char === '\t') {
        const matches = HELP_LINES.map(([cmd]) => cmd.split(' ')[0])
          .filter((cmd) => cmd.startsWith(line) && cmd !== line);
        if (matches.length === 1) replaceLine(matches[0]);
        continue;
      }
      if (char < ' ') continue;

      line += char;
      write(char);
    }
  };

  return {
    start: () => {
      write(`${BOLD}LightGuard Shell${RESET} ${DIM}· 本地模拟会话，未连接 SSH${RESET}\r\n`);
      write(`${DIM}输入 help 查看可用命令；主题切换会实时重绘终端配色。${RESET}\r\n\r\n`);
      write(prompt());
    },
    input,
    buffer: () => line,
  };
}
