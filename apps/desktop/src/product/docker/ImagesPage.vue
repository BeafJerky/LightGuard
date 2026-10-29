<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, watch, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type DockerImage } from './api';
import { dockerTime } from './format';
import { filterImages, imageReference, tagSummary } from './images';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';

const props = defineProps<{ server: Server }>();

// 弹窗挂到 Docker 应用根节点：.lgdocker-main 是查询容器，会改写绝对定位的包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const images = ref<DockerImage[]>([]);
const loading = ref(true);
const error = ref('');
const keyword = ref('');
const menuFor = ref('');

const action = useDockerWrite(props.server);
const {
  preview: actionPreview,
  busy: actionBusy,
  progress: actionProgress,
  error: actionError,
  done: actionDone,
  spent: actionSpent,
  ask: askAction,
  confirm: confirmAction,
  close: closeActionState,
} = action;

const visible = computed(() => filterImages(images.value, keyword.value));
const dangling = computed(() => images.value.filter((image) => image.dangling).length);
const summary = computed(() => {
  const base = `共 ${images.value.length} 个镜像`;
  return dangling.value > 0 ? `${base} · ${dangling.value} 个悬空` : base;
});

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    images.value = await dockerApi.images(props.server.id);
  } catch (caught) {
    error.value = toSshError(caught).message;
    images.value = [];
  } finally {
    loading.value = false;
  }
}

async function removeImage(image: DockerImage): Promise<void> {
  if (image.containers > 0) return;
  menuFor.value = '';
  const ready = await askAction({ kind: 'image.remove', target: imageReference(image) });
  if (!ready) toast({ title: '无法删除镜像', message: actionError.value, tone: 'error' });
}

function closeAction(): void {
  const finished = actionDone.value;
  closeActionState();
  if (finished?.result === 'success') void load();
}

function toggleMenu(id: string): void {
  menuFor.value = menuFor.value === id ? '' : id;
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null;
  if (target?.closest('.lgdocker-rowmenu')) return;
  menuFor.value = '';
}

watch(
  () => props.server.id,
  () => {
    menuFor.value = '';
    void load();
  },
);

onMounted(async () => {
  document.addEventListener('click', onDocumentClick);
  await load();
});

onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClick);
});
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>镜像管理</h2>
        <p>{{ summary }}</p>
      </div>
      <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
        <LgIcon name="refresh" :size="13" />
        刷新
      </button>
    </header>

    <div class="lgdocker-toolbar">
      <label class="lgdocker-search">
        <LgIcon name="search" :size="13" />
        <input v-model="keyword" type="search" placeholder="搜索仓库名或标签" aria-label="搜索镜像" />
      </label>
    </div>

    <div v-if="loading" class="lgdocker-empty">正在读取本地镜像…</div>

    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="15" />
      <div><strong>无法读取镜像</strong><span>{{ error }}</span></div>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>

    <div v-else-if="images.length === 0" class="lgdocker-empty">
      <strong>暂无本地镜像</strong>
      <span>在「镜像仓库」搜索并拉取镜像。</span>
    </div>

    <div v-else-if="visible.length === 0" class="lgdocker-empty">
      <strong>没有匹配的镜像</strong>
      <span>试试其它关键词。</span>
    </div>

    <div v-else class="lgdocker-imagelist">
      <article v-for="image in visible" :key="image.id" class="lgdocker-imagerow">
        <span class="lgdocker-imagerow__dot" :class="{ 'is-muted': image.dangling }" aria-hidden="true" />
        <span class="lgdocker-imagerow__icon">
          <LgIcon name="layers" :size="15" />
        </span>
        <div class="lgdocker-imagerow__main">
          <strong :title="image.repo || image.shortId">{{ image.repo || image.shortId }}</strong>
          <span class="lgdocker-imagerow__meta">
            <em>{{ dockerTime(image.createdAt) }}</em>
            <em>{{ tagSummary(image.tags) }}</em>
            <em>{{ image.size }}</em>
            <em v-if="image.containers > 0">{{ image.containers }} 个容器使用</em>
          </span>
        </div>
        <div class="lgdocker-rowmenu">
          <button
            type="button"
            class="lgdocker-iconbtn"
            :aria-label="`镜像 ${image.repo || image.shortId} 的操作`"
            @click.stop="toggleMenu(image.id)"
          >
            <LgIcon name="more" :size="13" />
          </button>
          <div v-if="menuFor === image.id" class="lgdocker-rowmenu__list" role="menu">
            <button
              type="button"
              role="menuitem"
              :disabled="image.containers > 0"
              :title="image.containers > 0 ? '正在被容器使用' : '删除镜像'"
              @click.stop="removeImage(image)"
            >
              {{ image.containers > 0 ? '正在被容器使用' : '删除镜像' }}
            </button>
          </div>
        </div>
      </article>
    </div>

    <footer v-if="!loading && !error" class="lgdocker-listfoot">{{ summary }}</footer>

    <Teleport v-if="actionPreview && dockerRoot" :to="dockerRoot">
      <DockerActionDialog
        :preview="actionPreview"
        :busy="actionBusy"
        :progress="actionProgress"
        :error="actionError"
        :done="actionDone"
        :spent="actionSpent"
        @confirm="confirmAction()"
        @close="closeAction"
      />
    </Teleport>
  </section>
</template>
