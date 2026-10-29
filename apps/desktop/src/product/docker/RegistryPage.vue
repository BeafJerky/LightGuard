<script setup lang="ts">
import { computed, inject, ref, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type RegistrySearchItem } from './api';
import { QUICK_REGISTRY, pullReference } from './images';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';

const props = defineProps<{ server: Server }>();

// 弹窗挂到 Docker 应用根节点：.lgdocker-main 是查询容器，会改写绝对定位的包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const term = ref('');
const results = ref<RegistrySearchItem[]>([]);
const searched = ref(false);
const searching = ref(false);
const error = ref('');

const pullOpen = ref(false);
const pullName = ref('');
const pullTag = ref('latest');
const cancelBusy = ref(false);

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

const pullRef = computed(() => pullReference(pullName.value, pullTag.value));

async function search(): Promise<void> {
  const keyword = term.value.trim();
  if (!keyword) return;
  searching.value = true;
  error.value = '';
  try {
    results.value = await dockerApi.registrySearch(props.server.id, keyword);
  } catch (caught) {
    error.value = toSshError(caught).message;
    results.value = [];
  } finally {
    searched.value = true;
    searching.value = false;
  }
}

function resetSearch(): void {
  term.value = '';
  results.value = [];
  searched.value = false;
  error.value = '';
}

function openPull(name: string): void {
  pullName.value = name.trim();
  if (!pullName.value) return;
  pullTag.value = 'latest';
  pullOpen.value = true;
}

function closePull(): void {
  pullOpen.value = false;
}

async function startPull(): Promise<void> {
  if (!pullName.value) return;
  const reference = pullRef.value;
  pullOpen.value = false;
  const ready = await askAction({ kind: 'image.pull', target: reference });
  if (!ready) toast({ title: '无法拉取镜像', message: actionError.value, tone: 'error' });
}

async function cancelPull(): Promise<void> {
  if (cancelBusy.value) return;
  cancelBusy.value = true;
  try {
    const canceled = await dockerApi.writeCancel(props.server.id);
    if (canceled) toast({ title: '已请求取消拉取', message: '正在停止当前拉取，已完成下载的层会保留。', tone: 'info' });
    else toast({ title: '没有进行中的拉取', tone: 'warning' });
  } catch (caught) {
    toast({ title: '取消失败', message: toSshError(caught).message, tone: 'error' });
  } finally {
    cancelBusy.value = false;
  }
}

function closeAction(): void {
  const finished = actionDone.value;
  closeActionState();
  if (finished?.result === 'success') {
    toast({ title: '镜像已拉取', message: finished.target, tone: 'success' });
  }
}
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>镜像仓库</h2>
        <p>搜索 Docker Hub，拉取到当前服务器</p>
      </div>
    </header>

    <div class="lgdocker-toolbar">
      <label class="lgdocker-search">
        <LgIcon name="search" :size="13" />
        <input
          v-model="term"
          type="search"
          placeholder="搜索镜像，例如 nginx"
          aria-label="搜索镜像仓库"
          @keydown.enter="search()"
        />
      </label>
      <button
        type="button"
        class="lgdocker-btn is-primary"
        :disabled="searching || !term.trim()"
        @click="search()"
      >
        <LgIcon name="search" :size="13" />
        搜索
      </button>
      <button v-if="searched" type="button" class="lgdocker-btn" :disabled="searching" @click="resetSearch">
        快速列表
      </button>
    </div>

    <div v-if="searching" class="lgdocker-empty">正在搜索镜像仓库…</div>

    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="15" />
      <div><strong>搜索失败</strong><span>{{ error }}</span></div>
      <button type="button" class="lgdocker-btn" @click="openPull(term.trim())">直接拉取</button>
    </div>

    <div v-else-if="!searched" class="lgdocker-quick">
      <h3>常用镜像</h3>
      <div class="lgdocker-quick__grid">
        <article v-for="item in QUICK_REGISTRY" :key="item.name" class="lgdocker-quick__item">
          <div>
            <strong>{{ item.name }}</strong>
            <span>{{ item.description }}</span>
          </div>
          <button type="button" class="lgdocker-btn is-small" @click="openPull(item.name)">拉取</button>
        </article>
      </div>
    </div>

    <div v-else-if="results.length === 0" class="lgdocker-empty">
      <strong>没有搜索结果</strong>
      <span>试试其它关键词，或直接拉取 {{ term.trim() }}。</span>
      <button type="button" class="lgdocker-btn" @click="openPull(term.trim())">直接拉取</button>
    </div>

    <div v-else class="lgdocker-table lgdocker-registry" role="table" aria-label="镜像搜索结果">
      <div class="lgdocker-table__head" role="row">
        <span>名称</span>
        <span>描述</span>
        <span>Stars</span>
        <span>Official</span>
        <span>操作</span>
      </div>
      <div v-for="item in results" :key="item.name" class="lgdocker-table__row" role="row">
        <span class="is-strong" :title="item.name">{{ item.name }}</span>
        <span :title="item.description">{{ item.description || '—' }}</span>
        <span class="is-num">{{ item.stars }}</span>
        <span>{{ item.official ? '官方' : '—' }}</span>
        <div class="lgdocker-table__actions">
          <button type="button" class="lgdocker-btn is-small" @click="openPull(item.name)">拉取</button>
        </div>
      </div>
    </div>

    <Teleport v-if="dockerRoot" :to="dockerRoot">
      <DockerActionDialog
        v-if="actionPreview"
        :preview="actionPreview"
        :busy="actionBusy"
        :progress="actionProgress"
        :error="actionError"
        :done="actionDone"
        :spent="actionSpent"
        cancellable
        @confirm="confirmAction()"
        @stop="cancelPull()"
        @close="closeAction"
      />

      <div v-if="pullOpen" class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="拉取镜像">
        <div class="lgdocker-modal__mask" @click="closePull" />
        <form class="lgdocker-modal__panel" @submit.prevent="startPull">
          <header class="lgdocker-modal__head">
            <div>
              <strong>拉取镜像</strong>
              <span>{{ pullRef }}</span>
            </div>
            <button type="button" class="lgdocker-btn is-small" aria-label="关闭拉取对话框" @click="closePull">
              <LgIcon name="x" :size="13" />
            </button>
          </header>
          <div class="lgdocker-modal__body">
            <dl class="lgdocker-modal__facts">
              <div><dt>镜像</dt><dd>{{ pullName }}</dd></div>
              <div><dt>引用</dt><dd>{{ pullRef }}</dd></div>
            </dl>
            <label class="lgdocker-field">
              <LgIcon name="layers" :size="13" />
              <input v-model="pullTag" type="text" placeholder="标签，默认 latest" aria-label="镜像标签" />
            </label>
            <p class="lgdocker-muted">
              拉取属于写操作，需要先解锁当前服务器；取消后已下载的层会保留。
            </p>
          </div>
          <footer class="lgdocker-modal__foot">
            <button type="button" class="lgdocker-btn" @click="closePull">取消</button>
            <button type="submit" class="lgdocker-btn is-primary">检查并拉取</button>
          </footer>
        </form>
      </div>
    </Teleport>
  </section>
</template>
