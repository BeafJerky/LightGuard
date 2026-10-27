import {
  defineComponent,
  h,
  inject,
  ref,
  type Component,
  type Ref,
  type Slots,
} from 'vue';
import {
  NAlert,
  NButton,
  NCard,
  NCheckbox,
  NDataTable,
  NDivider,
  NDropdown,
  NEmpty,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NModal,
  NPagination,
  NPopconfirm,
  NPopover,
  NProgress,
  NRadio,
  NRadioGroup,
  NSelect,
  NSkeleton,
  NSpin,
  NSwitch,
  NTabPane,
  NTabs,
  NTag,
  NTooltip,
} from 'naive-ui';
import { LG_SCOPE_KEY } from '../theme/context';

interface WrapperOptions {
  teleport?: boolean;
  defaults?: Record<string, unknown>;
  expose?: (inner: Ref<unknown>) => Record<string, unknown>;
}

/**
 * 薄封装：不改 Naive 的 API，只补三件事：
 * 1. 弹层默认 Teleport 到当前主题作用域，避免嵌套主题时弹层跑到 body；
 * 2. 统一默认值；
 * 3. 暴露 inner，复杂方法仍可触达。
 */
function createWrapper(name: string, component: Component, options: WrapperOptions = {}) {
  return defineComponent({
    name,
    inheritAttrs: false,
    setup(_, { attrs, slots, expose }) {
      const inner = ref<unknown>(null);
      const scope = inject(LG_SCOPE_KEY, null);
      expose(options.expose ? options.expose(inner) : { inner });

      return () => {
        const props: Record<string, unknown> = { ...options.defaults, ...attrs };
        if (options.teleport) {
          const to = attrs.to ?? scope?.value.portalEl;
          if (to) props.to = to;
          else delete props.to;
        }
        return h(component, { ...props, ref: inner }, slots as Slots);
      };
    },
  });
}

export const LgButton = createWrapper('LgButton', NButton);
export const LgInput = createWrapper('LgInput', NInput);
export const LgInputNumber = createWrapper('LgInputNumber', NInputNumber);
export const LgSelect = createWrapper('LgSelect', NSelect, { teleport: true });
export const LgSwitch = createWrapper('LgSwitch', NSwitch);
export const LgCheckbox = createWrapper('LgCheckbox', NCheckbox);
export const LgRadioGroup = createWrapper('LgRadioGroup', NRadioGroup);
export const LgRadio = createWrapper('LgRadio', NRadio);
export const LgTabs = createWrapper('LgTabs', NTabs);
// NTabs 通过 vnode.type.__TAB_PANE__ 识别子面板，任何包装层都会把它藏起来，因此这里直接复用。
export const LgTabPane = NTabPane;
export const LgDataTable = createWrapper('LgDataTable', NDataTable);
export const LgProgress = createWrapper('LgProgress', NProgress);
export const LgTag = createWrapper('LgTag', NTag);
export const LgAlert = createWrapper('LgAlert', NAlert);
export const LgCard = createWrapper('LgCard', NCard);
export const LgForm = createWrapper('LgForm', NForm, {
  expose: (inner) => ({
    inner,
    validate: (...args: unknown[]) => (inner.value as { validate?: (...a: unknown[]) => unknown } | null)?.validate?.(...args),
    restoreValidation: (...args: unknown[]) =>
      (inner.value as { restoreValidation?: (...a: unknown[]) => unknown } | null)?.restoreValidation?.(...args),
    feedback: (...args: unknown[]) =>
      (inner.value as { feedback?: (...a: unknown[]) => unknown } | null)?.feedback?.(...args),
  }),
});
export const LgFormItem = createWrapper('LgFormItem', NFormItem);
export const LgModal = createWrapper('LgModal', NModal, {
  teleport: true,
  defaults: { preset: 'card' },
});
export const LgTooltip = createWrapper('LgTooltip', NTooltip, { teleport: true });
export const LgDropdown = createWrapper('LgDropdown', NDropdown, { teleport: true });
export const LgPopover = createWrapper('LgPopover', NPopover, { teleport: true });
export const LgPopconfirm = createWrapper('LgPopconfirm', NPopconfirm, { teleport: true });
export const LgSpin = createWrapper('LgSpin', NSpin);
export const LgSkeleton = createWrapper('LgSkeleton', NSkeleton);
export const LgPagination = createWrapper('LgPagination', NPagination);
export const LgEmpty = createWrapper('LgEmpty', NEmpty);
export const LgDivider = createWrapper('LgDivider', NDivider);
