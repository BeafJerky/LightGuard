import { computed, inject, toValue } from 'vue';
import type { MaybeRefOrGetter } from 'vue';
import { LG_RENDER_KEY } from './context';
import type { RenderBackend, RenderHint, RenderTier } from './types';

export function useRenderBackend(
  tier: RenderTier,
  hint?: MaybeRefOrGetter<RenderHint | undefined>,
) {
  const context = inject(LG_RENDER_KEY, null);
  return computed<RenderBackend>(() => {
    if (!context) return 'dom';
    return context.value.resolve(tier, hint ? toValue(hint) : undefined);
  });
}

export function useRenderContext() {
  return inject(LG_RENDER_KEY, null);
}
