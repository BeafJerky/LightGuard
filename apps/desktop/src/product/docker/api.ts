import {
  SshCommandError,
  UNSUPPORTED_HINT,
  UNSUPPORTED_MESSAGE,
  type TerminalEvent,
} from '../ssh/backend';
import { call, isTauriRuntime } from '../ssh/tauri';

export interface DockerCapabilities {
  installed: boolean;
  daemonRunning: boolean;
  socketOk: boolean;
  /** 探测到 socket 权限不足（与「服务未运行」区分）：只能提示组权限，不能启动服务。 */
  permissionDenied: boolean;
  systemd: boolean;
  /** `systemctl is-enabled docker` 原始值：enabled / disabled / 空串。 */
  autoStart: string;
  compose: boolean;
  version: string;
  apiVersion: string;
  proxySupported: boolean;
  message: string;
}

export interface DockerInfo {
  version: string;
  apiVersion: string;
  storageDriver: string;
  dataRoot: string;
  mirrors: string[];
}

export interface DockerCounts {
  containersRunning: number;
  containersTotal: number;
  images: number;
}

export interface DockerDf {
  imagesSize: string;
  containersSize: string;
  volumesSize: string;
  buildCacheSize: string;
  reclaimable: string;
}

export interface DockerOverview {
  capabilities: DockerCapabilities;
  info: DockerInfo;
  counts: DockerCounts;
  df: DockerDf | null;
}

export interface DockerContainer {
  id: string;
  shortId: string;
  name: string;
  image: string;
  command: string;
  state: string;
  status: string;
  createdAt: string;
  project: string;
  service: string;
  ports: string;
  cpuPercent: number | null;
  memUsage: string;
  memLimit: string;
  memPercent: number | null;
  netRx: string;
  netTx: string;
}

export interface DockerStatsSample {
  id: string;
  cpuPercent: number;
  memUsage: string;
  memLimit: string;
  memPercent: number;
  netRx: string;
  netTx: string;
}

export interface DockerPort {
  hostIp: string;
  hostPort: string;
  containerPort: string;
  protocol: string;
}

export interface DockerMount {
  source: string;
  destination: string;
  mode: string;
  readOnly: boolean;
}

export interface DockerNetwork {
  name: string;
  ip: string;
  gateway: string;
  mac: string;
}

export interface DockerEnvVar {
  key: string;
  value: string;
}

export interface DockerContainerDetail {
  id: string;
  name: string;
  image: string;
  command: string;
  state: string;
  status: string;
  health: string;
  restartPolicy: string;
  createdAt: string;
  startedAt: string;
  ports: DockerPort[];
  mounts: DockerMount[];
  networks: DockerNetwork[];
  env: DockerEnvVar[];
  envTruncated: boolean;
}

export interface DockerImage {
  id: string;
  shortId: string;
  repo: string;
  tags: string[];
  size: string;
  createdAt: string;
  containers: number;
  dangling: boolean;
}

export interface RegistryConfig {
  mirrors: string[];
  httpProxy: string;
  httpsProxy: string;
  noProxy: string;
  proxyConfigured: boolean;
  proxySupported: boolean;
  readable: boolean;
  /** daemon.json 存在但不是合法 JSON 对象：禁止编辑。 */
  invalid: boolean;
  note: string;
}

export interface RegistrySearchItem {
  name: string;
  description: string;
  stars: number;
  official: boolean;
  automated: boolean;
}

export interface DockerPreview {
  previewId: string;
  kind: string;
  operation: string;
  target: string;
  impact: string;
  rollback: string;
  irreversible: boolean;
  expiresAtMs: number;
  details: string[];
}

export interface DockerWriteResult {
  auditId: number;
  target: string;
  result: 'success' | 'failed' | 'unknown';
  message: string;
}

export interface SudoStatus {
  passwordless: boolean;
  available: boolean;
}

export interface DockerWriteInput {
  kind: string;
  target: string;
}

export interface DockerSystemInput {
  kind: string;
  payload: Record<string, unknown>;
}

export interface ContainerTerminalInput {
  serverId: string;
  containerId: string;
  cols: number;
  rows: number;
  onEvent: (event: TerminalEvent) => void;
}

export interface DockerTerminalHandle {
  terminalId: string;
  title: string;
}

export interface DockerApi {
  readonly available: boolean;
  capabilities(serverId: string): Promise<DockerCapabilities>;
  overview(serverId: string, includeDf?: boolean): Promise<DockerOverview>;
  containers(serverId: string): Promise<DockerContainer[]>;
  containerDetail(serverId: string, containerId: string): Promise<DockerContainerDetail>;
  stats(serverId: string): Promise<DockerStatsSample[]>;
  images(serverId: string): Promise<DockerImage[]>;
  registryConfig(serverId: string): Promise<RegistryConfig>;
  registrySearch(serverId: string, term: string, limit?: number): Promise<RegistrySearchItem[]>;
  writePreview(serverId: string, input: DockerWriteInput): Promise<DockerPreview>;
  writeExecute(previewId: string, onLine: (line: string) => void): Promise<DockerWriteResult>;
  writeCancel(serverId: string): Promise<boolean>;
  systemPreview(serverId: string, input: DockerSystemInput, password?: string): Promise<DockerPreview>;
  systemExecute(
    previewId: string,
    password?: string,
    onLine?: (line: string) => void,
  ): Promise<DockerWriteResult>;
  sudoStatus(serverId: string): Promise<SudoStatus>;
  openContainerTerminal(input: ContainerTerminalInput): Promise<DockerTerminalHandle>;
}

export function createTauriDockerApi(): DockerApi {
  return {
    available: true,
    capabilities(serverId) {
      return call('docker_capabilities', { serverId });
    },
    overview(serverId, includeDf = true) {
      return call('docker_overview', { serverId, includeDf });
    },
    containers(serverId) {
      return call('docker_containers', { serverId });
    },
    containerDetail(serverId, containerId) {
      return call('docker_container_detail', { serverId, containerId });
    },
    stats(serverId) {
      return call('docker_stats', { serverId });
    },
    images(serverId) {
      return call('docker_images', { serverId });
    },
    registryConfig(serverId) {
      return call('docker_registry_config', { serverId });
    },
    registrySearch(serverId, term, limit = 25) {
      return call('docker_registry_search', { serverId, term, limit });
    },
    writePreview(serverId, input) {
      return call('docker_write_preview', { serverId, input });
    },
    async writeExecute(previewId, onLine) {
      const { Channel } = await import('@tauri-apps/api/core');
      const channel = new Channel<{ line: string }>();
      channel.onmessage = (event) => {
        if (event.line) onLine(event.line);
      };
      return call('docker_write_execute', { previewId, onEvent: channel });
    },
    writeCancel(serverId) {
      return call('docker_write_cancel', { serverId });
    },
    systemPreview(serverId, input, password) {
      return call('docker_system_preview', { serverId, input, password });
    },
    async systemExecute(previewId, password, onLine) {
      const { Channel } = await import('@tauri-apps/api/core');
      const channel = new Channel<{ line: string }>();
      channel.onmessage = (event) => {
        if (event.line) onLine?.(event.line);
      };
      return call('docker_system_execute', { previewId, password, onEvent: channel });
    },
    sudoStatus(serverId) {
      return call('docker_sudo_status', { serverId });
    },
    async openContainerTerminal(input) {
      const { Channel } = await import('@tauri-apps/api/core');
      const channel = new Channel<TerminalEvent>();
      const decoder = new TextDecoder();
      channel.onmessage = (event) => {
        if (event.type === 'data') {
          const bytes = Uint8Array.from(atob(event.data), (char) => char.charCodeAt(0));
          input.onEvent({ type: 'data', data: decoder.decode(bytes, { stream: true }) });
          return;
        }
        input.onEvent(event);
      };
      const opened = await call<{ terminalId: string; title: string }>(
        'docker_container_terminal_open',
        {
          input: {
            serverId: input.serverId,
            containerId: input.containerId,
            cols: input.cols,
            rows: input.rows,
          },
          onEvent: channel,
        },
      );
      return opened;
    },
  };
}

export function createUnsupportedDockerApi(): DockerApi {
  const reject = <T,>(): Promise<T> =>
    Promise.reject(new SshCommandError('UNSUPPORTED', UNSUPPORTED_MESSAGE, UNSUPPORTED_HINT));
  return {
    available: false,
    capabilities: reject,
    overview: reject,
    containers: reject,
    containerDetail: reject,
    stats: reject,
    images: reject,
    registryConfig: reject,
    registrySearch: reject,
    writePreview: reject,
    writeExecute: reject,
    writeCancel: reject,
    systemPreview: reject,
    systemExecute: reject,
    sudoStatus: reject,
    openContainerTerminal: reject,
  };
}

export const dockerApi: DockerApi = isTauriRuntime()
  ? createTauriDockerApi()
  : createUnsupportedDockerApi();
