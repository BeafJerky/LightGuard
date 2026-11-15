<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, watch, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { dismissToast, toast } from '../ui';
import { dockerApi, type DockerImage } from './api';
import { dockerTime } from './format';
import { filterImages, imageReference, tagSummary } from './images';
import { permissionHint } from './status';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';
import BuildImageDialog from './BuildImageDialog.vue';

const props = defineProps<{ server: Server }>();

// 弹窗挂到 Docker 应用根节点：.lgdocker-main 是查询容器，会改写绝对定位的包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const images = ref<DockerImage[]>([]);
const loading = ref(true);
const error = ref('');
const errorCode = ref('');
const keyword = ref('');
const menuFor = ref('');
const exporting = ref('');
const checking = ref('');
const buildOpen = ref(false);

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
  errorCode.value = '';
  try {
    images.value = await dockerApi.images(props.server.id);
  } catch (caught) {
    const sshError = toSshError(caught);
    error.value = sshError.message;
    errorCode.value = sshError.code;
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

async function pushImage(image: DockerImage): Promise<void> {
  if (image.dangling || image.tags.length === 0) return;
  menuFor.value = '';
  const ready = await askAction({ kind: 'image.push', target: imageReference(image) });
  if (!ready) toast({ title: '无法推送镜像', message: actionError.value, tone: 'error' });
}

async function importImage(): Promise<void> {
  try {
    const path = await dockerApi.pickArchive();
    if (!path) return;
    const ready = await askAction({ kind: 'image.import', target: path });
    if (!ready) toast({ title: '无法导入镜像', message: actionError.value, tone: 'error' });
  } catch (caught) {
    toast({ title: '无法选择归档', message: toSshError(caught).message, tone: 'error' });
  }
}

async function pruneDangling(): Promise<void> {
  if (dangling.value === 0) {
    toast({ title: '没有悬空镜像', tone: 'info' });
    return;
  }
  const ready = await askAction({ kind: 'image.prune', target: '' });
  if (!ready) toast({ title: '无法清理镜像', message: actionError.value, tone: 'error' });
}

async function exportImage(image: DockerImage): Promise<void> {
  menuFor.value = '';
  if (exporting.value) return;
  exporting.value = image.id;
  const pending = toast({
    title: '正在导出镜像',
    message: imageReference(image),
    duration: 0,
  });
  try {
    const path = await dockerApi.exportImage(
      props.server.id,
      image.id,
      `img-${Date.now()}`,
      () => {},
    );
    toast({
      title: '镜像已导出',
      message: path,
      tone: 'success',
    });
  } catch (caught) {
    toast({ title: '导出失败', message: toSshError(caught).message, tone: 'error' });
  } finally {
    dismissToast(pending);
    exporting.value = '';
  }
}

async function checkUpdate(image: DockerImage): Promise<void> {
  menuFor.value = '';
  if (checking.value) return;
  checking.value = image.id;
  const pending = toast({
    title: '正在检测更新',
    message: imageReference(image),
    duration: 0,
  });
  try {
    const status = await dockerApi.imageUpdate(props.server.id, image.id);
    const title =
      status.state === 'current'
        ? '已是最新'
        : status.state === 'behind'
          ? '发现新版本'
          : '无法确认更新';
    toast({
      title,
      message:
        status.state === 'unknown'
          ? status.message
          : `${status.reference} ${status.message}`,
      tone: status.state === 'behind' ? 'warning' : status.state === 'current' ? 'success' : 'info',
    });
  } catch (caught) {
    toast({ title: '检测失败', message: toSshError(caught).message, tone: 'error' });
  } finally {
    dismissToast(pending);
    checking.value = '';
  }
}

async function buildImage(payload: {
  contextPath: string;
  tag: string;
  dockerfile: string;
}): Promise<void> {
  buildOpen.value = false;
  const ready = await askAction({
    kind: 'image.build',
    target: payload.tag,
    payload: { contextPath: payload.contextPath, dockerfile: payload.dockerfile },
  });
  if (!ready) toast({ title: '无法构建镜像', message: actionError.value, tone: 'error' });
}

function closeAction(): void {
  const finished = actionDone.value;
  const kind = actionPreview.value?.kind ?? '';
  closeActionState();
  if (finished?.result === 'success') {
    const title =
      kind === 'image.build'
        ? '镜像构建完成'
        : kind === 'image.import'
          ? '镜像已导入'
          : kind === 'image.push'
            ? '镜像已推送'
            : kind === 'image.prune'
              ? '悬空镜像已清理'
              : kind === 'image.remove'
                ? '镜像已删除'
                : '操作完成';
    toast({ title, message: finished.target, tone: 'success' });
    void load();
  }
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
      <div class="lgdocker-pagehead__actions">
        <button type="button" class="lgdocker-btn" :disabled="loading" @click="importImage()">
          <LgIcon name="download" :size="13" />
          导入
        </button>
        <button type="button" class="lgdocker-btn" :disabled="loading" @click="buildOpen = true">
          <LgIcon name="layers" :size="13" />
          构建
        </button>
        <button
          type="button"
          class="lgdocker-btn"
          :disabled="loading || dangling === 0"
          :title="dangling === 0 ? '没有悬空镜像' : '删除未打标签且没有容器引用的镜像'"
          @click="pruneDangling()"
        >
          <LgIcon name="trash" :size="13" />
          清理悬空
        </button>
        <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
          <LgIcon name="refresh" :size="13" />
          刷新
        </button>
      </div>
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
      <div>
        <strong>无法读取镜像</strong>
        <span>{{ error }}</span>
        <span v-if="permissionHint(errorCode)">{{ permissionHint(errorCode) }}</span>
      </div>
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
              :disabled="image.dangling || image.tags.length === 0"
              @click.stop="pushImage(image)"
            >
              推送镜像
            </button>
            <button type="button" role="menuitem" @click.stop="exportImage(image)">
              {{ exporting === image.id ? '正在导出…' : '导出为 tar.gz' }}
            </button>
            <button
              type="button"
              role="menuitem"
              :disabled="image.dangling || Boolean(checking)"
              @click.stop="checkUpdate(image)"
            >
              {{ checking === image.id ? '正在检测…' : '检测更新' }}
            </button>
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

    <Teleport v-if="buildOpen && dockerRoot" :to="dockerRoot">
      <BuildImageDialog @close="buildOpen = false" @build="buildImage" />
    </Teleport>
  </section>
</template>
