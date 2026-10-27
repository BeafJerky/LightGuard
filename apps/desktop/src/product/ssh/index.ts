import type { SshBackend } from './backend';
import { tauriBackend } from './tauri';
import { unsupportedBackend } from './unsupported';

export * from './backend';
export * from './adapter';

export const ssh: SshBackend = tauriBackend.available ? tauriBackend : unsupportedBackend;
export const backendAvailable = ssh.available;
