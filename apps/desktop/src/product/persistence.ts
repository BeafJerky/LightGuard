import { normalizeRenderSettings } from '@lightguard/lightui';
import type { RenderSettings } from '@lightguard/lightui';
import { isTauriRuntime } from './ssh/tauri';
import { toast } from './ui';
import type {
  AppSettings,
  BackupSummary,
  LogEntry,
  PersistedServer,
  ThemeChoices,
} from './types';

export const LEGACY_STORE_KEY = 'lightguard.app.v1';
export const LEGACY_BACKUP_KEY = 'lightguard.backup.last';
export const LEGACY_RENDER_KEY = 'lg.render.v1';

export interface StoredState {
  initialized: boolean;
  themes: ThemeChoices | null;
  settings: AppSettings | null;
  servers: PersistedServer[];
  logs: LogEntry[];
  backup: BackupSummary | null;
  render: RenderSettings | null;
}

export interface StoredSnapshot {
  themes: ThemeChoices;
  settings: AppSettings;
  servers: PersistedServer[];
  logs: LogEntry[];
  backup: BackupSummary | null;
  render?: RenderSettings | null;
}

export type StoredOp =
  | { op: 'serverUpsert'; server: PersistedServer }
  | { op: 'serverDelete'; id: string }
  | { op: 'logAppend'; log: LogEntry }
  | { op: 'logsClear' }
  | { op: 'themesSet'; themes: ThemeChoices }
  | { op: 'settingsSet'; settings: AppSettings }
  | { op: 'backupSet'; summary: BackupSummary | null }
  | { op: 'renderSet'; settings: RenderSettings }
  | { op: 'replace'; snapshot: StoredSnapshot }
  | { op: 'reset' };

interface RawStoredState {
  initialized?: unknown;
  themes?: unknown;
  settings?: unknown;
  servers?: unknown;
  logs?: unknown;
  backup?: unknown;
  render?: unknown;
}

type CoreModule = typeof import('@tauri-apps/api/core');

let coreModule: Promise<CoreModule> | null = null;
const loadCore = () => (coreModule ??= import('@tauri-apps/api/core'));

let active = false;
let loaded: StoredState | null = null;
let warned = false;
let queue: Promise<void> = Promise.resolve();

export function isPersistenceActive() {
  return active;
}

export function loadedStoredState() {
  return loaded;
}

function warnOnce(error: unknown) {
  if (warned) return;
  warned = true;
  console.warn('[lightguard] SQLite persistence unavailable, using in-memory state', error);
  toast({
    title: '本地数据库不可用',
    message: '本次运行使用临时存储，重启后可能丢失改动。',
    tone: 'warning',
    duration: 8000,
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function normalizeStored(raw: RawStoredState): StoredState {
  return {
    initialized: raw.initialized === true,
    themes: isRecord(raw.themes) ? (raw.themes as unknown as ThemeChoices) : null,
    settings: isRecord(raw.settings) ? (raw.settings as unknown as AppSettings) : null,
    servers: Array.isArray(raw.servers) ? (raw.servers as PersistedServer[]) : [],
    logs: Array.isArray(raw.logs) ? (raw.logs as LogEntry[]) : [],
    backup: isRecord(raw.backup) ? (raw.backup as unknown as BackupSummary) : null,
    render: isRecord(raw.render)
      ? normalizeRenderSettings(raw.render)
      : null,
  };
}

export async function loadStoredState(): Promise<StoredState | null> {
  if (!isTauriRuntime()) return null;
  try {
    const { invoke } = await loadCore();
    const raw = await invoke<RawStoredState>('db_load');
    loaded = normalizeStored(raw ?? {});
    return loaded;
  } catch (error) {
    warnOnce(error);
    return null;
  }
}

export function activatePersistence() {
  active = true;
}

function runStrict(op: StoredOp): Promise<void> {
  const task = async () => {
    const { invoke } = await loadCore();
    await invoke('db_write', { op });
  };
  const result = queue.then(task, task);
  queue = result.catch((error) => {
    warnOnce(error);
  });
  return result;
}

export function writePersistence(op: StoredOp): void {
  if (!active || !isTauriRuntime()) return;
  void runStrict(op).catch(() => {
    /* warned once inside the queue */
  });
}

export async function writePersistenceNow(op: StoredOp): Promise<void> {
  if (!active || !isTauriRuntime()) return;
  await runStrict(op);
}

export async function replaceStoredState(snapshot: StoredSnapshot): Promise<void> {
  await runStrict({ op: 'replace', snapshot });
  if (loaded) loaded.initialized = true;
  else loaded = { initialized: true, themes: snapshot.themes, settings: snapshot.settings, servers: snapshot.servers, logs: snapshot.logs, backup: snapshot.backup, render: snapshot.render ?? null };
}

export async function resetStoredState(): Promise<void> {
  if (!isTauriRuntime()) return;
  await runStrict({ op: 'reset' });
}

export function clearLegacyAppKeys() {
  try {
    window.localStorage.removeItem(LEGACY_STORE_KEY);
    window.localStorage.removeItem(LEGACY_BACKUP_KEY);
  } catch {
    /* storage can be unavailable in private mode */
  }
}

export function clearLegacyRenderKey() {
  try {
    window.localStorage.removeItem(LEGACY_RENDER_KEY);
  } catch {
    /* storage can be unavailable in private mode */
  }
}

export function hasLegacyRender(): boolean {
  try {
    return typeof localStorage !== 'undefined' && localStorage.getItem(LEGACY_RENDER_KEY) !== null;
  } catch {
    return false;
  }
}
