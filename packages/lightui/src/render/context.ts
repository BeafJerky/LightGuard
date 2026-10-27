import type { ComputedRef, InjectionKey } from 'vue';
import type { RenderScopeContext } from './types';

export const LG_RENDER_KEY: InjectionKey<ComputedRef<RenderScopeContext>> = Symbol('lg-render');
