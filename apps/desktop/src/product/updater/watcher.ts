export const UPDATE_STARTUP_DELAY_MS = 10_000;
export const UPDATE_INTERVAL_MS = 24 * 60 * 60 * 1000;

export interface UpdateChecker {
  check(options?: { manual?: boolean }): Promise<void>;
}

export function startUpdateWatcher(
  checker: UpdateChecker,
  options: { delayMs?: number; intervalMs?: number } = {},
): () => void {
  const delay = window.setTimeout(() => {
    void checker.check({ manual: false });
  }, options.delayMs ?? UPDATE_STARTUP_DELAY_MS);
  const timer = window.setInterval(() => {
    void checker.check({ manual: false });
  }, options.intervalMs ?? UPDATE_INTERVAL_MS);

  return () => {
    window.clearTimeout(delay);
    window.clearInterval(timer);
  };
}
