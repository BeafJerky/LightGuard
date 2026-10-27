import type { ComputedRef, InjectionKey } from 'vue';
import type { ThemeScopeContext } from './types';

export const LG_SCOPE_KEY: InjectionKey<ComputedRef<ThemeScopeContext>> = Symbol('lg-scope');
