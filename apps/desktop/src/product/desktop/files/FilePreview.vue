<script setup lang="ts">
import { computed } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { FileEntry } from '../../ssh/backend';
import { fileTypeLabel, formatSize, formatTime } from './format';
import type { PreviewState } from './useFiles';

const props = defineProps<{ preview: PreviewState; entry: FileEntry | null }>();
const emit = defineEmits<{ close: []; retry: []; download: [] }>();

const dataUrl = computed(() =>
  props.preview.image ? `data:${props.preview.image.mime};base64,${props.preview.image.base64}` : '',
);
</script>

<template>
  <aside class="lgfiles-preview" aria-label="预览与属性">
    <header class="lgfiles-preview__head">
      <strong>{{ entry?.name ?? '预览' }}</strong>
      <span class="lgfiles-preview__actions">
        <button v-if="entry && entry.kind !== 'dir'" type="button" aria-label="下载" @click="emit('download')">
          <LgIcon name="download" :size="14" />
        </button>
        <button type="button" aria-label="关闭预览" @click="emit('close')">
          <LgIcon name="x" :size="14" />
        </button>
      </span>
    </header>

    <div v-if="preview.loading" class="lgfiles-preview__state">正在读取…</div>
    <div v-else-if="preview.error" class="lgfiles-preview__state is-error">
      <span>{{ preview.error.message }}</span>
      <button type="button" @click="emit('retry')">重试</button>
    </div>
    <template v-else>
      <div v-if="preview.mode === 'text' && preview.text" class="lgfiles-preview__text">
        <div class="lgfiles-preview__badges">
          <span>{{ preview.text.encoding.toUpperCase() }}</span>
          <span v-if="preview.text.truncated">仅显示前 1MB</span>
        </div>
        <pre>{{ preview.text.content }}</pre>
      </div>
      <div v-else-if="preview.mode === 'image' && preview.image" class="lgfiles-preview__image">
        <img :src="dataUrl" :alt="entry?.name ?? '图片预览'" />
        <small>{{ formatSize(preview.image.bytes) }}</small>
      </div>
      <div v-else class="lgfiles-preview__meta-note">
        <LgIcon name="info" :size="14" />
        该类型只显示属性，不读取文件内容。
      </div>
    </template>

    <dl v-if="entry" class="lgfiles-preview__props">
      <div><dt>名称</dt><dd>{{ entry.name }}</dd></div>
      <div><dt>完整路径</dt><dd>{{ entry.path }}</dd></div>
      <div><dt>类型</dt><dd>{{ fileTypeLabel(entry) }}</dd></div>
      <div><dt>大小</dt><dd>{{ entry.kind === 'dir' ? '—' : formatSize(entry.size) }}</dd></div>
      <div><dt>修改时间</dt><dd>{{ formatTime(entry.mtime) }}</dd></div>
      <div><dt>权限</dt><dd>{{ entry.modeText }}</dd></div>
      <div><dt>UID / GID</dt><dd>{{ entry.uid ?? '—' }} / {{ entry.gid ?? '—' }}</dd></div>
      <div v-if="entry.symlinkTarget"><dt>链接指向</dt><dd>{{ entry.symlinkTarget }}</dd></div>
    </dl>
  </aside>
</template>
