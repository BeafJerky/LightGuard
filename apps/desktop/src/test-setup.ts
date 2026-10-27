// Node 24 exposes an experimental localStorage global that can shadow jsdom's
// implementation, so tests get a small deterministic storage instead.
const memory = new Map<string, string>();

const storage: Storage = {
  get length() {
    return memory.size;
  },
  clear() {
    memory.clear();
  },
  getItem(key: string) {
    return memory.has(key) ? (memory.get(key) as string) : null;
  },
  key(index: number) {
    return Array.from(memory.keys())[index] ?? null;
  },
  removeItem(key: string) {
    memory.delete(key);
  },
  setItem(key: string, value: string) {
    memory.set(key, String(value));
  },
};

Object.defineProperty(window, 'localStorage', {
  configurable: true,
  value: storage,
});
