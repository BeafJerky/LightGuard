<script setup lang="ts">
import { computed, inject } from 'vue';
import { LG_SCOPE_KEY } from '@lightguard/lightui';
import {
  APP_GLYPHS,
  APP_ICON_SIZES,
  appIconScheme,
  appTileColor,
  schemeBTile,
  type AppIconKey,
} from './appIcons';

const props = withDefaults(
  defineProps<{
    app: AppIconKey;
    tile: string;
    size?: number;
    glyph?: number;
    radius?: 'sm' | 'md';
  }>(),
  { size: APP_ICON_SIZES.taskbar.tile, glyph: APP_ICON_SIZES.taskbar.glyph, radius: 'sm' },
);

const scope = inject(LG_SCOPE_KEY, null);
const theme = computed(() => scope?.value.theme);
const scheme = computed(() => (theme.value ? appIconScheme(theme.value.id) : 'a'));
const parts = computed(() => APP_GLYPHS[props.app]);

const style = computed(() => {
  const base = {
    width: `${props.size}px`,
    height: `${props.size}px`,
    '--app-icon-glyph-size': `${props.glyph}px`,
  };
  if (!theme.value || scheme.value !== 'b') return base;
  const tile = schemeBTile(appTileColor(theme.value, props.tile), theme.value);
  return { ...base, '--app-icon-ink': tile.ink };
});
</script>

<template>
  <span
    class="app-icon"
    :class="[tile, `app-icon--${scheme}`, radius === 'md' && 'app-icon--radius-md']"
    :style="style"
    aria-hidden="true"
  >
    <svg class="app-icon__glyph" viewBox="0 0 24 24">
      <path
        v-for="(part, index) in parts"
        :key="index"
        :d="part.d"
        :fill="part.mode === 'stroke' ? 'none' : 'currentColor'"
        :fill-rule="part.mode === 'stroke' ? undefined : 'evenodd'"
        :stroke="part.mode === 'stroke' ? 'currentColor' : undefined"
        :stroke-width="part.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </span>
</template>

<style scoped>
.app-icon {
  display: grid;
  flex: 0 0 auto;
  box-sizing: border-box;
  place-items: center;
  overflow: hidden;
  border-radius: var(--lg-radius-sm);
}

.app-icon--radius-md {
  border-radius: var(--lg-radius-md);
}

.app-icon__glyph {
  display: block;
  width: var(--app-icon-glyph-size);
  height: var(--app-icon-glyph-size);
}

/* 方案 A：完全沿用旧色块配方，仅替换字形。 */
.app-icon--a {
  border: var(--lg-border-width) solid color-mix(in srgb, var(--tile-color) 34%, var(--lg-border));
  background: linear-gradient(
    135deg,
    color-mix(in srgb, var(--tile-color) 36%, var(--lg-surface-2)),
    color-mix(in srgb, var(--tile-color) 15%, var(--lg-surface))
  );
  color: var(--tile-color);
  box-shadow:
    var(--lg-shadow-sm),
    inset 0 1px 0 color-mix(in srgb, var(--lg-surface) 28%, transparent);
}

/* 方案 B：高饱和主题色块，墨色由 schemeBTile 按对比度选定。 */
.app-icon--b {
  border: var(--lg-border-width) solid color-mix(in srgb, var(--tile-color) 46%, var(--lg-border));
  background:
    radial-gradient(120% 84% at 24% 8%, rgba(255, 255, 255, 0.14), transparent 58%),
    linear-gradient(
      135deg,
      color-mix(in srgb, var(--tile-color) 70%, var(--lg-surface-2)),
      color-mix(in srgb, var(--tile-color) 60%, var(--lg-surface))
    );
  color: var(--app-icon-ink);
  box-shadow:
    var(--lg-shadow-sm),
    inset 0 1px 0 color-mix(in srgb, var(--lg-surface) 30%, transparent);
}
</style>
