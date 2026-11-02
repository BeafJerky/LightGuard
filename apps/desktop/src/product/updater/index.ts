import { UpdateController } from './controller';
import { createTauriBridge } from './tauri';

export * from './types';
export { UpdateController, MAX_BUFFERED_BYTES, classifyUpdateError } from './controller';
export { startUpdateWatcher, UPDATE_INTERVAL_MS, UPDATE_STARTUP_DELAY_MS } from './watcher';

export const updateController = new UpdateController(createTauriBridge());
