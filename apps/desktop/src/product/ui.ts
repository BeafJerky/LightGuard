import { reactive, ref } from 'vue';

export type ToastTone = 'success' | 'info' | 'warning' | 'error';

export interface ToastInput {
  title: string;
  message?: string;
  tone?: ToastTone;
  duration?: number;
}

export interface Toast extends ToastInput {
  id: number;
}

export const toasts = reactive<Toast[]>([]);
const timers = new Map<number, number>();
let nextToastId = 1;

export function dismissToast(id: number) {
  const index = toasts.findIndex((toast) => toast.id === id);
  if (index >= 0) toasts.splice(index, 1);
  const timer = timers.get(id);
  if (timer) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
}

export function toast(input: ToastInput | string, tone: ToastTone = 'info'): number {
  const value: ToastInput = typeof input === 'string' ? { title: input, tone } : input;
  const id = nextToastId++;
  toasts.push({ id, tone: value.tone ?? 'info', duration: value.duration ?? 3600, ...value });
  if (toasts.length > 4) dismissToast(toasts[0].id);
  const duration = value.duration ?? 3600;
  if (duration > 0) {
    timers.set(
      id,
      window.setTimeout(() => dismissToast(id), duration),
    );
  }
  return id;
}

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: 'default' | 'danger';
}

interface ConfirmState extends ConfirmOptions {
  resolve: (value: boolean) => void;
}

export const confirmState = ref<ConfirmState | null>(null);

export function confirm(options: ConfirmOptions): Promise<boolean> {
  if (confirmState.value) confirmState.value.resolve(false);
  return new Promise((resolve) => {
    confirmState.value = { ...options, resolve };
  });
}

export function resolveConfirm(value: boolean) {
  const current = confirmState.value;
  if (!current) return;
  confirmState.value = null;
  current.resolve(value);
}

export const wizardOpen = ref(false);

export function openWizard() {
  wizardOpen.value = true;
}
