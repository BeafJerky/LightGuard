import { APP_VERSION, isCloseBehavior, isDesktopBarMode } from './data';
import type {
  AppSettings,
  AuthKind,
  BackupConnection,
  BackupEnvelope,
  BackupPayload,
  Server,
  ThemeChoices,
} from './types';

export const BACKUP_FORMAT = 'lightguard-backup';
export const BACKUP_VERSION = 1;
export const MAX_BACKUP_BYTES = 5 * 1024 * 1024;
export const MAX_BACKUP_CONNECTIONS = 200;
const KDF_ITERATIONS = 600000;
const AUTH_KINDS: AuthKind[] = ['key', 'password', 'agent'];

export type BackupErrorCode =
  | 'FILE_TOO_LARGE'
  | 'INVALID_FORMAT'
  | 'UNSUPPORTED_VERSION'
  | 'INVALID_FIELD'
  | 'DECRYPT_FAILED'
  | 'CRYPTO_UNAVAILABLE'
  | 'READ_FAILED';

export class BackupError extends Error {
  code: BackupErrorCode;

  constructor(code: BackupErrorCode, message: string) {
    super(message);
    this.name = 'BackupError';
    this.code = code;
  }
}

export interface BackupFileInfo {
  envelope: BackupEnvelope;
  encrypted: boolean;
  createdAt: string;
  size: number;
}

export interface BackupPreview {
  payload: BackupPayload;
  encrypted: boolean;
  createdAt: string;
  warnings: string[];
  duplicates: BackupConnection[];
  duplicateCount: number;
  settingsCount: number;
  credentialsIncluded: false;
}

export interface CreateBackupInput {
  settings: AppSettings;
  themes: ThemeChoices;
  servers: Server[];
  includeSettings: boolean;
  includeConnections: boolean;
  encrypt: boolean;
  password?: string;
}

export interface CreateBackupResult {
  json: string;
  fileName: string;
  hosts: number;
  encrypted: boolean;
}

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();

function bytesToBase64(bytes: Uint8Array) {
  let binary = '';
  const chunk = 0x8000;
  for (let index = 0; index < bytes.length; index += chunk) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunk));
  }
  return btoa(binary);
}

function base64ToBytes(value: string) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function assertSafe(value: unknown, depth = 0) {
  if (depth > 8) throw new BackupError('INVALID_FIELD', '备份中包含无法识别的数据');
  if (Array.isArray(value)) {
    for (const item of value) assertSafe(item, depth + 1);
    return;
  }
  if (!isRecord(value)) return;
  for (const key of Object.keys(value)) {
    if (key === '__proto__' || key === 'prototype' || key === 'constructor') {
      throw new BackupError('INVALID_FIELD', '备份中包含无法识别的数据');
    }
    assertSafe(value[key], depth + 1);
  }
}

function readString(
  source: Record<string, unknown>,
  key: string,
  label: string,
  max: number,
  required = true,
): string {
  const value = source[key];
  if (value === undefined || value === null) {
    if (!required) return '';
    throw new BackupError('INVALID_FIELD', `备份中的${label}缺失`);
  }
  if (typeof value !== 'string') throw new BackupError('INVALID_FIELD', `备份中的${label}格式无效`);
  const trimmed = value.trim();
  if (trimmed.length > max) throw new BackupError('INVALID_FIELD', `备份中的${label}超出长度限制`);
  if (required && !trimmed) throw new BackupError('INVALID_FIELD', `备份中的${label}为空`);
  return trimmed;
}

function readBool(source: Record<string, unknown>, key: string, fallback: boolean) {
  const value = source[key];
  return typeof value === 'boolean' ? value : fallback;
}

function normalizeSettings(value: unknown): AppSettings {
  if (!isRecord(value)) throw new BackupError('INVALID_FIELD', '备份中的设置格式无效');
  const interval = Number(value.interval);
  const idleLock = Number(value.idleLock);
  return {
    interval: Number.isFinite(interval) && interval >= 2 && interval <= 60 ? Math.round(interval) : 5,
    autoRefresh: readBool(value, 'autoRefresh', true),
    hostKey: readBool(value, 'hostKey', true),
    clipboardClear: readBool(value, 'clipboardClear', true),
    notifyFailure: readBool(value, 'notifyFailure', true),
    notifyThreshold: readBool(value, 'notifyThreshold', true),
    idleLock: Number.isFinite(idleLock) && idleLock >= 0 && idleLock <= 240 ? Math.round(idleLock) : 30,
    closeBehavior: isCloseBehavior(value.closeBehavior) ? value.closeBehavior : 'ask',
    desktopBar: isDesktopBarMode(value.desktopBar) ? value.desktopBar : 'floating',
  };
}

function normalizeThemes(value: unknown, warnings: string[]): ThemeChoices {
  if (!isRecord(value)) return { console: 'dark', desktop: 'pixel' };
  const allowed = ['dark', 'light', 'claude', 'pixel', 'nes', 'tech', 'system'];
  const read = (key: 'console' | 'desktop', fallback: ThemeChoices['console']) => {
    const raw = value[key];
    if (typeof raw !== 'string') return fallback;
    if (allowed.includes(raw)) return raw as ThemeChoices['console'];
    warnings.push(`未知主题 ${raw} 已回退为 ${fallback}`);
    return fallback;
  };
  return {
    console: read('console', 'dark'),
    desktop: read('desktop', 'pixel'),
  };
}

function normalizeConnection(value: unknown, index: number): BackupConnection {
  if (!isRecord(value)) throw new BackupError('INVALID_FIELD', `第 ${index + 1} 条连接格式无效`);
  const port = Number(value.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new BackupError('INVALID_FIELD', `第 ${index + 1} 条连接的端口无效`);
  }
  const authRaw = value.auth;
  const auth: AuthKind = typeof authRaw === 'string' && AUTH_KINDS.includes(authRaw as AuthKind)
    ? (authRaw as AuthKind)
    : 'key';
  const tagsRaw = Array.isArray(value.tags) ? value.tags : [];
  if (tagsRaw.length > 24) throw new BackupError('INVALID_FIELD', `第 ${index + 1} 条连接的标签过多`);
  const tags = tagsRaw.map((tag) => {
    if (typeof tag !== 'string' || tag.length > 20) {
      throw new BackupError('INVALID_FIELD', `第 ${index + 1} 条连接包含无效标签`);
    }
    return tag.trim();
  });
  return {
    id: readString(value, 'id', '连接 ID', 80),
    name: readString(value, 'name', '连接名称', 80),
    role: readString(value, 'role', '连接角色', 80, false),
    ip: readString(value, 'ip', '主机地址', 255),
    port,
    user: readString(value, 'user', '登录用户', 64),
    auth,
    group: readString(value, 'group', '分组', 40, false),
    tags,
    note: readString(value, 'note', '备注', 500, false),
  };
}

export function validatePayload(value: unknown): { payload: BackupPayload; warnings: string[] } {
  assertSafe(value);
  if (!isRecord(value)) throw new BackupError('INVALID_FIELD', '备份数据格式无效');
  const warnings: string[] = [];
  const scope = isRecord(value.scope) ? value.scope : {};
  const connectionsRaw = value.connections;
  if (!Array.isArray(connectionsRaw)) throw new BackupError('INVALID_FIELD', '备份中的连接列表无效');
  if (connectionsRaw.length > MAX_BACKUP_CONNECTIONS) {
    throw new BackupError('FILE_TOO_LARGE', `备份包含超过 ${MAX_BACKUP_CONNECTIONS} 台连接`);
  }
  const connections = connectionsRaw.map(normalizeConnection);
  const appVersion = typeof value.appVersion === 'string' ? value.appVersion.slice(0, 32) : '未知';
  return {
    payload: {
      appVersion,
      scope: {
        settings: readBool(scope, 'settings', false),
        connections: readBool(scope, 'connections', connections.length > 0),
      },
      settings: normalizeSettings(value.settings ?? {}),
      themes: normalizeThemes(value.themes, warnings),
      connections,
    },
    warnings,
  };
}

export function buildBackupPayload(input: {
  settings: AppSettings;
  themes: ThemeChoices;
  servers: Server[];
  includeSettings: boolean;
  includeConnections: boolean;
}): BackupPayload {
  const connections: BackupConnection[] = input.includeConnections
    ? input.servers.slice(0, MAX_BACKUP_CONNECTIONS).map((server) => ({
        id: server.id,
        name: server.name,
        role: server.role,
        ip: server.ip,
        port: server.port,
        user: server.user,
        auth: server.auth,
        group: server.group,
        tags: [...server.tags],
        note: server.note,
      }))
    : [];
  return {
    appVersion: APP_VERSION,
    scope: { settings: input.includeSettings, connections: input.includeConnections },
    settings: { ...input.settings },
    themes: { ...input.themes },
    connections,
  };
}

async function deriveKey(password: string, salt: Uint8Array, iterations: number) {
  if (!globalThis.crypto?.subtle) {
    throw new BackupError('CRYPTO_UNAVAILABLE', '当前环境不支持加密备份');
  }
  const baseKey = await crypto.subtle.importKey(
    'raw',
    textEncoder.encode(password),
    'PBKDF2',
    false,
    ['deriveKey'],
  );
  return crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt: salt as unknown as BufferSource,
      iterations,
      hash: 'SHA-256',
    },
    baseKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  );
}

function backupFileName(encrypted: boolean) {
  const stamp = new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-');
  return `lightguard-backup-${stamp}${encrypted ? '-encrypted' : ''}.lgbackup`;
}

export async function createBackup(input: CreateBackupInput): Promise<CreateBackupResult> {
  if (!input.includeSettings && !input.includeConnections) {
    throw new BackupError('INVALID_FIELD', '请至少选择一项备份范围');
  }
  const payload = buildBackupPayload(input);
  const createdAt = new Date().toISOString();
  let envelope: BackupEnvelope;
  if (input.encrypt) {
    const password = input.password ?? '';
    if (password.length < 8) throw new BackupError('INVALID_FIELD', '加密口令至少需要 8 个字符');
    const salt = crypto.getRandomValues(new Uint8Array(16));
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const key = await deriveKey(password, salt, KDF_ITERATIONS);
    const cipherBuffer = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv: iv as unknown as BufferSource },
      key,
      textEncoder.encode(JSON.stringify(payload)),
    );
    envelope = {
      format: BACKUP_FORMAT,
      version: BACKUP_VERSION,
      createdAt,
      encrypted: true,
      kdf: { name: 'PBKDF2-SHA256', iterations: KDF_ITERATIONS, salt: bytesToBase64(salt) },
      cipher: { name: 'AES-256-GCM', iv: bytesToBase64(iv) },
      payload: bytesToBase64(new Uint8Array(cipherBuffer)),
    };
  } else {
    envelope = {
      format: BACKUP_FORMAT,
      version: BACKUP_VERSION,
      createdAt,
      encrypted: false,
      payload,
    };
  }
  const json = JSON.stringify(envelope, null, 2);
  if (new Blob([json]).size > MAX_BACKUP_BYTES) {
    throw new BackupError('FILE_TOO_LARGE', '备份超过 5MB，请缩小范围后重新导出');
  }
  return {
    json,
    fileName: backupFileName(input.encrypt),
    hosts: payload.connections.length,
    encrypted: input.encrypt,
  };
}

function parseEnvelope(value: unknown): BackupEnvelope {
  assertSafe(value);
  if (!isRecord(value)) throw new BackupError('INVALID_FORMAT', '这不是有效的 LightGuard 备份文件');
  if (value.format !== BACKUP_FORMAT) {
    throw new BackupError('INVALID_FORMAT', '这不是有效的 LightGuard 备份文件');
  }
  if (value.version !== BACKUP_VERSION) {
    throw new BackupError('UNSUPPORTED_VERSION', '备份版本不受支持，请升级 LightGuard');
  }
  if (typeof value.encrypted !== 'boolean') {
    throw new BackupError('INVALID_FORMAT', '备份文件缺少加密标记');
  }
  const createdAt = typeof value.createdAt === 'string' ? value.createdAt : new Date().toISOString();
  if (value.encrypted) {
    if (!isRecord(value.kdf) || !isRecord(value.cipher)) {
      throw new BackupError('INVALID_FORMAT', '备份文件缺少解密参数');
    }
    const iterations = Number(value.kdf.iterations);
    if (
      value.kdf.name !== 'PBKDF2-SHA256' ||
      !Number.isFinite(iterations) ||
      iterations < 100000 ||
      iterations > 2000000
    ) {
      throw new BackupError('INVALID_FORMAT', '备份文件的密钥派生参数无效');
    }
    if (value.cipher.name !== 'AES-256-GCM') {
      throw new BackupError('INVALID_FORMAT', '备份文件的加密算法无效');
    }
    if (typeof value.kdf.salt !== 'string' || typeof value.cipher.iv !== 'string') {
      throw new BackupError('INVALID_FORMAT', '备份文件的解密参数无效');
    }
    if (typeof value.payload !== 'string' || !value.payload) {
      throw new BackupError('INVALID_FORMAT', '备份文件的密文缺失');
    }
    return {
      format: BACKUP_FORMAT,
      version: BACKUP_VERSION,
      createdAt,
      encrypted: true,
      kdf: {
        name: 'PBKDF2-SHA256',
        iterations,
        salt: value.kdf.salt,
      },
      cipher: { name: 'AES-256-GCM', iv: value.cipher.iv },
      payload: value.payload,
    };
  }
  return {
    format: BACKUP_FORMAT,
    version: BACKUP_VERSION,
    createdAt,
    encrypted: false,
    payload: value.payload as BackupPayload,
  };
}

export async function readBackupFile(file: File): Promise<BackupFileInfo> {
  if (file.size > MAX_BACKUP_BYTES) {
    throw new BackupError('FILE_TOO_LARGE', '备份文件超过 5MB 限制');
  }
  let text: string;
  try {
    text = await file.text();
  } catch {
    throw new BackupError('READ_FAILED', '无法读取备份文件');
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new BackupError('INVALID_FORMAT', '这不是有效的 LightGuard 备份文件');
  }
  const envelope = parseEnvelope(parsed);
  return {
    envelope,
    encrypted: envelope.encrypted,
    createdAt: envelope.createdAt,
    size: file.size,
  };
}

export async function decryptBackup(envelope: BackupEnvelope, password: string) {
  if (!envelope.encrypted || !envelope.kdf || !envelope.cipher) {
    throw new BackupError('INVALID_FORMAT', '备份文件未加密');
  }
  let salt: Uint8Array;
  let iv: Uint8Array;
  let cipher: Uint8Array;
  try {
    salt = base64ToBytes(envelope.kdf.salt);
    iv = base64ToBytes(envelope.cipher.iv);
    cipher = base64ToBytes(envelope.payload as string);
  } catch {
    throw new BackupError('INVALID_FORMAT', '备份文件的解密参数无效');
  }
  if (salt.length < 16 || salt.length > 64 || iv.length !== 12) {
    throw new BackupError('INVALID_FORMAT', '备份文件的解密参数无效');
  }
  try {
    const key = await deriveKey(password, salt, envelope.kdf.iterations);
    const plain = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: iv as unknown as BufferSource },
      key,
      cipher as unknown as BufferSource,
    );
    const parsed = JSON.parse(textDecoder.decode(plain));
    return validatePayload(parsed);
  } catch (error) {
    if (error instanceof BackupError) throw error;
    throw new BackupError('DECRYPT_FAILED', '口令错误或文件已损坏');
  }
}

export function getPreview(
  payload: BackupPayload,
  warnings: string[],
  encrypted: boolean,
  createdAt: string,
  servers: Server[],
): BackupPreview {
  const duplicates = payload.connections.filter((connection) =>
    servers.some(
      (server) =>
        server.id === connection.id ||
        (server.ip === connection.ip &&
          Number(server.port) === Number(connection.port) &&
          server.user === connection.user),
    ),
  );
  const settingsCount = Object.keys(payload.settings).length;
  return {
    payload,
    encrypted,
    createdAt,
    warnings,
    duplicates: duplicates.slice(0, 3),
    duplicateCount: duplicates.length,
    settingsCount,
    credentialsIncluded: false,
  };
}

export function downloadBackup(fileName: string, json: string) {
  try {
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = fileName;
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  } catch {
    throw new BackupError('READ_FAILED', '当前浏览器不允许本地下载');
  }
}

export function summaryFromBackup(hosts: number, encrypted: boolean) {
  return {
    at: new Date().toISOString(),
    hosts,
    encrypted,
  };
}

export function backupConnectionKey(connection: BackupConnection) {
  return `${connection.ip}:${connection.port} · ${connection.user}`;
}
