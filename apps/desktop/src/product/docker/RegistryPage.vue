<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type RegistrySearchItem, type RegistryTags } from './api';
import { QUICK_REGISTRY, pullReference } from './images';
import { permissionHint, registryHint, tagHint } from './status';
import { TAG_RENDER_LIMIT, displayRepo, tagView } from './tags';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';
import RegistrySettings from './RegistrySettings.vue';

const props = defineProps<{ server: Server }>();

// 弹窗挂到 Docker 应用根节点：.lgdocker-main 是查询容器，会改写绝对定位的包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const term = ref('');
const searchOpen = ref(false);
const results = ref<RegistrySearchItem[]>([]);
const searched = ref(false);
const searching = ref(false);
const error = ref('');
const errorCode = ref('');

const tagsData = ref<RegistryTags | null>(null);
const tagsLoading = ref(false);
const tagsError = ref('');
const tagsErrorCode = ref('');
const tagFilter = ref('');

const pullOpen = ref(false);
const pullName = ref('');
const pullTag = ref('latest');
// 拉取弹窗的版本下拉：打开即预取 tags/list，和查版本面板共用同一份缓存。
const pullTagMenu = ref(false);
const pullTagFilter = ref('');
const pullTagActive = ref(-1);
const pullTagInput = ref<HTMLInputElement | null>(null);
const pullTags = ref<string[]>([]);
const pullTagsLoading = ref(false);
const pullTagsError = ref('');
const pullTagsRepo = ref('');
const pullTagsCache = new Map<string, string[]>();
const cancelBusy = ref(false);
const settingsMenu = ref(false);
const settingsOpen = ref(false);
const settingsMode = ref<'mirrors' | 'proxy'>('mirrors');

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
const tagRepo = computed(() => (tagsData.value ? displayRepo(tagsData.value.repo) : ''));
const tags = computed(() => tagView(tagsData.value?.tags ?? [], tagFilter.value));
const pullOptions = computed(() => tagView(pullTags.value, pullTagFilter.value));

/** 搜索与查版本互斥：进入任一路径都先清掉另一边的结果与错误。 */
function clearViews(): void {
  results.value = [];
  searched.value = false;
  error.value = '';
  errorCode.value = '';
  tagsData.value = null;
  tagsLoading.value = false;
  tagsError.value = '';
  tagsErrorCode.value = '';
  tagFilter.value = '';
}

async function search(): Promise<void> {
  const keyword = term.value.trim();
  if (!keyword) return;
  clearViews();
  searchOpen.value = true;
  searching.value = true;
  try {
    results.value = await dockerApi.registrySearch(props.server.id, keyword);
  } catch (caught) {
    const sshError = toSshError(caught);
    error.value = sshError.message;
    errorCode.value = sshError.code;
    results.value = [];
  } finally {
    searched.value = true;
    searching.value = false;
  }
}

/** 查版本只读加速源上的 tags/list，不经过 Docker Hub。 */
async function loadTags(): Promise<void> {
  const keyword = term.value.trim();
  if (!keyword) return;
  clearViews();
  searchOpen.value = true;
  tagsLoading.value = true;
  try {
    tagsData.value = await dockerApi.registryTags(props.server.id, keyword);
  } catch (caught) {
    const sshError = toSshError(caught);
    tagsError.value = sshError.message;
    tagsErrorCode.value = sshError.code;
  } finally {
    tagsLoading.value = false;
  }
}

function resetSearch(): void {
  term.value = '';
  clearViews();
  searchOpen.value = false;
}

/** 版本缓存按「去掉标签/摘要的仓库名」归类，和查版本面板共用同一份数据。 */
function repoKey(name: string): string {
  const raw = name.trim().toLowerCase();
  const digest = raw.indexOf('@');
  const plain = digest >= 0 ? raw.slice(0, digest) : raw;
  const slash = plain.lastIndexOf('/');
  const colon = plain.lastIndexOf(':');
  return colon > slash ? plain.slice(0, colon) : plain;
}

function usePullTags(list: string[], key: string): void {
  pullTags.value = list;
  pullTagsRepo.value = key;
  pullTagsError.value = '';
  pullTagsCache.set(key, list);
}

/** 同一仓库已经查过版本时直接复用，避免弹窗再次请求加速源。 */
function seedPullTags(name: string): boolean {
  const key = repoKey(name);
  if (pullTagsRepo.value === key && !pullTagsError.value) return true;
  const cached = pullTagsCache.get(key);
  if (cached) {
    usePullTags(cached, key);
    return true;
  }
  if (tagsData.value && displayRepo(tagsData.value.repo).toLowerCase() === key) {
    usePullTags(tagsData.value.tags, key);
    return true;
  }
  return false;
}

/** 版本列表只是只读查询：失败时不阻塞弹窗，仍可手动输入标签。 */
async function loadPullTags(): Promise<void> {
  const name = pullName.value.trim();
  if (!name || seedPullTags(name)) return;
  const key = repoKey(name);
  pullTagsRepo.value = key;
  pullTags.value = [];
  pullTagsLoading.value = true;
  pullTagsError.value = '';
  try {
    const data = await dockerApi.registryTags(props.server.id, name);
    if (pullTagsRepo.value !== key) return;
    usePullTags(data.tags, key);
  } catch (caught) {
    if (pullTagsRepo.value !== key) return;
    pullTagsError.value = toSshError(caught).message;
  } finally {
    if (pullTagsRepo.value === key) pullTagsLoading.value = false;
  }
}

function openPullTagMenu(): void {
  pullTagMenu.value = true;
  pullTagFilter.value = '';
  pullTagActive.value = -1;
  if (pullTags.value.length === 0 && !pullTagsLoading.value && !pullTagsError.value) {
    void loadPullTags();
  }
}

function togglePullTagMenu(): void {
  if (pullTagMenu.value) {
    pullTagMenu.value = false;
    return;
  }
  openPullTagMenu();
}

function onPullTagInput(): void {
  pullTagMenu.value = true;
  pullTagFilter.value = pullTag.value;
  // 输入时不高亮任何版本：回车仍然是提交手动输入的标签。
  pullTagActive.value = -1;
  if (pullTags.value.length === 0 && !pullTagsLoading.value && !pullTagsError.value) {
    void loadPullTags();
  }
}

function choosePullTag(tag: string): void {
  pullTag.value = tag;
  pullTagMenu.value = false;
  pullTagFilter.value = '';
  pullTagActive.value = -1;
  void nextTick(() => pullTagInput.value?.focus());
}

function movePullTag(step: number): void {
  if (!pullTagMenu.value) openPullTagMenu();
  const count = pullOptions.value.shown.length;
  if (count === 0) return;
  const current = pullTagActive.value;
  pullTagActive.value =
    current < 0 ? (step > 0 ? 0 : count - 1) : (current + step + count) % count;
}

function onPullTagKeydown(event: KeyboardEvent): void {
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    movePullTag(event.key === 'ArrowDown' ? 1 : -1);
    return;
  }
  if (event.key === 'Escape' && pullTagMenu.value) {
    event.preventDefault();
    pullTagMenu.value = false;
    return;
  }
  if (event.key === 'Enter' && pullTagMenu.value && pullTagActive.value >= 0) {
    const chosen = pullOptions.value.shown[pullTagActive.value];
    if (chosen) {
      event.preventDefault();
      choosePullTag(chosen);
    }
  }
}

function onPullTagFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (next && (event.currentTarget as HTMLElement | null)?.contains(next)) return;
  pullTagMenu.value = false;
}

function openPull(name: string, tag = 'latest'): void {
  pullName.value = name.trim();
  if (!pullName.value) return;
  pullTag.value = tag;
  pullTagMenu.value = false;
  pullTagFilter.value = '';
  pullTagActive.value = -1;
  pullTags.value = [];
  pullTagsLoading.value = false;
  pullTagsError.value = '';
  pullTagsRepo.value = '';
  pullOpen.value = true;
  void loadPullTags();
}

function closePull(): void {
  pullOpen.value = false;
  pullTagMenu.value = false;
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
    if (canceled) {
      toast({
        title: '已请求取消拉取',
        message: '正在停止当前拉取，已下载的层会保留。',
        tone: 'info',
      });
    }
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

function openSettings(mode: 'mirrors' | 'proxy'): void {
  settingsMenu.value = false;
  settingsMode.value = mode;
  settingsOpen.value = true;
}

function onDocumentClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null;
  if (target?.closest('.lgdocker-settingsmenu')) return;
  settingsMenu.value = false;
}

onMounted(() => document.addEventListener('click', onDocumentClick));
onBeforeUnmount(() => document.removeEventListener('click', onDocumentClick));
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>镜像仓库</h2>
        <p>搜索 Docker Hub，拉取到当前服务器</p>
      </div>
      <div class="lgdocker-pagehead__actions">
        <div class="lgdocker-rowmenu lgdocker-settingsmenu">
          <button
            type="button"
            class="lgdocker-btn"
            aria-haspopup="menu"
            :aria-expanded="settingsMenu"
            @click.stop="settingsMenu = !settingsMenu"
          >
            <LgIcon name="settings" :size="13" />
            设置
          </button>
          <div v-if="settingsMenu" class="lgdocker-rowmenu__list is-settings" role="menu">
            <button type="button" role="menuitem" @click.stop="openSettings('mirrors')">
              <LgIcon name="download" :size="14" />
              加速源设置
            </button>
            <button type="button" role="menuitem" @click.stop="openSettings('proxy')">
              <LgIcon name="network" :size="14" />
              代理设置
            </button>
          </div>
        </div>
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
        :disabled="searching || tagsLoading || !term.trim()"
        @click="search()"
      >
        <LgIcon name="search" :size="13" />
        搜索
      </button>
      <button
        type="button"
        class="lgdocker-btn"
        :disabled="searching || tagsLoading || !term.trim()"
        title="从加速源列出该镜像的所有版本，可拉取旧版本"
        @click="loadTags()"
      >
        <LgIcon name="layers" :size="13" />
        查版本
      </button>
      <button v-if="searchOpen" type="button" class="lgdocker-btn" :disabled="searching || tagsLoading" @click="resetSearch">
        快速列表
      </button>
    </div>

    <div v-if="tagsLoading" class="lgdocker-empty">正在查询 {{ term.trim() }} 的版本…</div>

    <div v-else-if="tagsError" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="15" />
      <div>
        <strong>查询版本失败</strong>
        <span>{{ tagsError }}</span>
        <span v-if="permissionHint(tagsErrorCode)">{{ permissionHint(tagsErrorCode) }}</span>
        <span v-else-if="tagHint(tagsErrorCode)">{{ tagHint(tagsErrorCode) }}</span>
      </div>
      <div class="lgdocker-error__actions">
        <button type="button" class="lgdocker-btn" @click="loadTags()">重试</button>
        <button type="button" class="lgdocker-btn" @click="openPull(term.trim())">直接拉取</button>
      </div>
    </div>

    <div v-else-if="tagsData" class="lgdocker-tagpanel">
      <div class="lgdocker-tagpanel__head">
        <div class="lgdocker-tagpanel__title">
          <strong>{{ tagRepo }}</strong>
          <span>共 {{ tagsData.tags.length }} 个版本</span>
        </div>
        <label class="lgdocker-search is-compact">
          <LgIcon name="search" :size="13" />
          <input
            v-model="tagFilter"
            type="search"
            placeholder="过滤版本，例如 alpine"
            aria-label="过滤镜像版本"
          />
        </label>
      </div>

      <div v-if="tags.matched.length === 0" class="lgdocker-empty">
        <strong>{{ tagsData.tags.length === 0 ? '加速源没有返回版本' : '没有匹配的版本' }}</strong>
        <span v-if="tagsData.tags.length === 0">该镜像可能没有同步到加速源，换个名字或直接拉取试试。</span>
        <span v-else>继续输入缩小范围，或清空过滤条件。</span>
      </div>

      <div v-else class="lgdocker-table lgdocker-tagtable" role="table" aria-label="镜像版本列表">
        <div class="lgdocker-table__head" role="row">
          <span>版本</span>
          <span>操作</span>
        </div>
        <div v-for="tag in tags.shown" :key="tag" class="lgdocker-table__row" role="row">
          <span class="is-strong" :title="tag">{{ tag }}</span>
          <div class="lgdocker-table__actions">
            <button type="button" class="lgdocker-btn is-small" @click="openPull(tagRepo, tag)">
              拉取
            </button>
          </div>
        </div>
      </div>

      <p v-if="tags.truncated" class="lgdocker-muted">
        仅显示前 {{ TAG_RENDER_LIMIT }} 个，共匹配 {{ tags.matched.length }} 个，继续输入缩小范围。
      </p>
    </div>

    <div v-else-if="searching" class="lgdocker-empty">正在搜索镜像仓库…最长等待 30 秒</div>

    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="15" />
      <div>
        <strong>搜索失败</strong>
        <span>{{ error }}</span>
        <span v-if="permissionHint(errorCode)">{{ permissionHint(errorCode) }}</span>
        <span v-else-if="registryHint(errorCode)">{{ registryHint(errorCode) }}</span>
      </div>
      <div class="lgdocker-error__actions">
        <button type="button" class="lgdocker-btn" @click="search()">重试</button>
        <button type="button" class="lgdocker-btn" @click="openPull(term.trim())">直接拉取</button>
      </div>
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
            <div class="lgdocker-tagcombo" @focusout="onPullTagFocusOut">
              <div class="lgdocker-field">
                <LgIcon name="layers" :size="13" />
                <input
                  ref="pullTagInput"
                  v-model="pullTag"
                  type="text"
                  placeholder="标签，默认 latest"
                  aria-label="镜像标签"
                  role="combobox"
                  aria-autocomplete="list"
                  aria-controls="lgdocker-pull-tags"
                  :aria-expanded="pullTagMenu"
                  :aria-activedescendant="pullTagActive >= 0 ? `lgdocker-pull-tag-${pullTagActive}` : undefined"
                  @input="onPullTagInput"
                  @keydown="onPullTagKeydown"
                />
                <button
                  type="button"
                  class="lgdocker-tagcombo__toggle"
                  aria-label="选择镜像版本"
                  :aria-expanded="pullTagMenu"
                  @click="togglePullTagMenu"
                >
                  <LgIcon :name="pullTagMenu ? 'chevron-up' : 'chevron-down'" :size="13" />
                </button>
              </div>

              <div
                v-if="pullTagMenu"
                id="lgdocker-pull-tags"
                class="lgdocker-tagcombo__list"
                role="listbox"
                aria-label="镜像版本"
              >
                <p v-if="pullTagsLoading" class="lgdocker-tagcombo__hint">正在获取版本…</p>
                <div v-else-if="pullTagsError" class="lgdocker-tagcombo__hint is-error">
                  <span>{{ pullTagsError }}</span>
                  <button type="button" class="lgdocker-btn is-small" @click="loadPullTags()">重试</button>
                </div>
                <p v-else-if="pullOptions.matched.length === 0" class="lgdocker-tagcombo__hint">
                  {{ pullTags.length === 0 ? '加速源没有返回版本，仍可直接输入标签。' : '没有匹配的版本' }}
                </p>
                <template v-else>
                  <button
                    v-for="(tag, index) in pullOptions.shown"
                    :id="`lgdocker-pull-tag-${index}`"
                    :key="tag"
                    type="button"
                    role="option"
                    class="lgdocker-tagcombo__option"
                    :class="{ 'is-active': index === pullTagActive }"
                    :aria-selected="tag === pullTag"
                    @mousedown.prevent
                    @mouseenter="pullTagActive = index"
                    @click="choosePullTag(tag)"
                  >
                    {{ tag }}
                  </button>
                  <p v-if="pullOptions.truncated" class="lgdocker-tagcombo__hint">
                    仅显示前 {{ TAG_RENDER_LIMIT }} 个，共匹配 {{ pullOptions.matched.length }} 个，输入可继续过滤。
                  </p>
                </template>
              </div>
            </div>
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

    <RegistrySettings
      :server="server"
      :open="settingsOpen"
      :mode="settingsMode"
      @close="settingsOpen = false"
      @applied="settingsOpen = false"
    />
  </section>
</template>
