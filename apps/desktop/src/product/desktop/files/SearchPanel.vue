<script setup lang="ts">
import { ref, watch } from 'vue';
import { LgButton, LgIcon, LgInput, LgModal } from '@lightguard/lightui';
import { ssh, toSshError, type FileEntry } from '../../ssh';

const props = defineProps<{
  open: boolean;
  serverId: string;
  serverName: string;
  root: string;
}>();

const emit = defineEmits<{
  close: [];
  open: [entry: FileEntry];
}>();

const query = ref('');
const results = ref<FileEntry[]>([]);
const loading = ref(false);
const truncated = ref(false);
const error = ref('');
const searched = ref(false);

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    query.value = '';
    results.value = [];
    truncated.value = false;
    error.value = '';
    searched.value = false;
  },
);

async function runSearch() {
  const term = query.value.trim();
  if (!term || loading.value) return;
  loading.value = true;
  error.value = '';
  try {
    const result = await ssh.sftpSearch({
      serverId: props.serverId,
      root: props.root,
      query: term,
    });
    results.value = result.entries;
    truncated.value = result.truncated;
    searched.value = true;
  } catch (caught) {
    error.value = toSshError(caught).message;
  } finally {
    loading.value = false;
  }
}

function openEntry(entry: FileEntry) {
  emit('open', entry);
}

function parentOf(path: string): string {
  const index = path.lastIndexOf('/');
  return index <= 0 ? '/' : path.slice(0, index);
}
</script>

<template>
  <LgModal
    :show="open"
    preset="card"
    class="search-panel"
    :bordered="false"
    :close-on-esc="true"
    title="递归搜索"
    @update:show="(value: boolean) => { if (!value) emit('close'); }"
  >
    <div class="search-panel__body">
      <p class="search-panel__host">{{ serverName }} · {{ root }}</p>
      <div class="search-panel__form">
        <LgInput
          v-model:value="query"
          :input-props="{ 'aria-label': '搜索文件', autocomplete: 'off', spellcheck: false }"
          placeholder="输入文件名关键字"
          @keydown.enter="runSearch"
        />
        <LgButton size="small" type="primary" data-action="search" :loading="loading" @click="runSearch">
          <template #icon><LgIcon name="search" :size="13" /></template>
          搜索
        </LgButton>
      </div>
      <p class="search-panel__note">最多向下 8 层、500 条结果；不跟随符号链接。</p>

      <p v-if="error" class="search-panel__error" role="alert">{{ error }}</p>
      <p v-else-if="searched && !results.length" class="search-panel__empty">
        没有匹配的文件或文件夹。
      </p>
      <ul v-if="results.length" class="search-panel__results">
        <li v-for="entry in results" :key="entry.path">
          <button type="button" @click="openEntry(entry)">
            <span class="search-panel__name">{{ entry.name }}</span>
            <span class="search-panel__path">{{ parentOf(entry.path) }}</span>
            <span class="search-panel__meta">{{ entry.kind === 'dir' ? '文件夹' : entry.size + ' B' }}</span>
          </button>
        </li>
      </ul>
      <p v-if="truncated" class="search-panel__note">
        已达结果上限，缩小关键字或范围后重试。
      </p>
    </div>
  </LgModal>
</template>

<style scoped>
.search-panel {
  width: min(620px, calc(100vw - 28px));
}

.search-panel__body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.search-panel__host {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
}

.search-panel__form {
  display: flex;
  gap: 8px;
  align-items: center;
}

.search-panel__note {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-xs, 12px);
}

.search-panel__empty,
.search-panel__error {
  margin: 0;
  font-size: var(--lg-fs-sm);
}

.search-panel__error {
  color: var(--lg-danger);
}

.search-panel__results {
  display: flex;
  flex-direction: column;
  max-height: 320px;
  margin: 0;
  padding: 0;
  overflow: auto;
  list-style: none;
}

.search-panel__results button {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) auto;
  gap: 10px;
  width: 100%;
  padding: 7px 6px;
  border: 0;
  border-bottom: 1px solid var(--lg-border);
  background: none;
  color: var(--lg-text);
  font: inherit;
  font-size: var(--lg-fs-sm);
  text-align: left;
  cursor: pointer;
}

.search-panel__results button:hover {
  background: var(--lg-surface);
}

.search-panel__name,
.search-panel__path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.search-panel__path,
.search-panel__meta {
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-xs, 12px);
}
</style>