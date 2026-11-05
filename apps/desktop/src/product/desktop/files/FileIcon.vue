<script setup lang="ts">
import { computed } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { FileEntry } from '../../ssh/backend';
import { fileCategory, fileColorIcon, fileIcon, fileTone } from './format';
import type { ResolvedIconScheme } from './iconScheme';

const props = defineProps<{
  entry: FileEntry;
  scheme: ResolvedIconScheme;
  size: number;
}>();

const colorIcon = computed(() => (props.scheme === 'color' ? fileColorIcon(props.entry) : null));
const lineIcon = computed(() => fileIcon(props.entry));
const tone = computed(() => fileTone(props.entry));
const isFolder = computed(() => fileCategory(props.entry) === 'folder');
</script>

<template>
  <svg
    v-if="colorIcon"
    class="lgfiles-file-icon is-color"
    :width="size"
    :height="size"
    viewBox="0 0 32 32"
    aria-hidden="true"
  >
    <use :href="`#lg-fv-${colorIcon}`" />
  </svg>
  <LgIcon
    v-else
    class="lgfiles-file-icon"
    :class="[`is-tone-${tone}`, { 'is-folder': isFolder }]"
    :name="lineIcon"
    :size="size"
  />
</template>
