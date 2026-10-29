<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toast } from '../ui';
import { openPermitPanel, permitOf } from '../guard/store';
import OverviewPage from './OverviewPage.vue';
import ContainersPage from './ContainersPage.vue';
import ImagesPage from './ImagesPage.vue';
import RegistryPage from './RegistryPage.vue';
import './docker.css';

const props = defineProps<{ server: Server }>();

type PageKey = 'overview' | 'containers' | 'compose' | 'images' | 'registry' | 'network';

interface NavItem {
  key: PageKey;
  label: string;
  icon: string;
  soon?: boolean;
}

const nav: NavItem[] = [
  { key: 'overview', label: '概览', icon: 'grid' },
  { key: 'containers', label: '容器', icon: 'cartridge' },
  { key: 'compose', label: 'Compose', icon: 'menu', soon: true },
  { key: 'images', label: '本地镜像', icon: 'image' },
  { key: 'registry', label: '镜像仓库', icon: 'globe' },
  { key: 'network', label: '网络', icon: 'network', soon: true },
];

const page = ref<PageKey>('overview');
const root = ref<HTMLElement | null>(null);
const narrow = ref(false);
const locked = computed(() => permitOf(props.server.id).locked);
let observer: ResizeObserver | null = null;

onMounted(() => {
  if (!root.value || typeof ResizeObserver === 'undefined') return;
  observer = new ResizeObserver((entries) => {
    const width = entries[0]?.contentRect.width ?? 0;
    narrow.value = width > 0 && width < 760;
  });
  observer.observe(root.value);
});

onBeforeUnmount(() => observer?.disconnect());

function select(item: NavItem) {
  if (item.soon) {
    toast({ title: '该模块将在后续版本开放', message: `${item.label} 不在 V0.13 范围内。`, tone: 'info' });
    return;
  }
  page.value = item.key;
}
</script>

<template>
  <div ref="root" class="lgdocker" :class="{ 'is-narrow': narrow }">
    <aside class="lgdocker-nav">
      <div class="lgdocker-brand">
        <span class="lgdocker-brand__icon" aria-hidden="true">
          <LgIcon name="cartridge" :size="16" />
        </span>
        <div>
          <strong>容器</strong>
          <span>{{ props.server.name || props.server.ip }}</span>
        </div>
      </div>
      <nav class="lgdocker-nav__list" aria-label="容器导航">
        <button
          v-for="item in nav"
          :key="item.key"
          type="button"
          class="lgdocker-nav__item"
          :class="{ 'is-active': page === item.key, 'is-soon': item.soon }"
          :aria-current="page === item.key ? 'page' : undefined"
          @click="select(item)"
        >
          <LgIcon :name="item.icon" :size="14" />
          <span>{{ item.label }}</span>
          <em v-if="item.soon">后续开放</em>
        </button>
      </nav>
      <p class="lgdocker-nav__foot">Docker 只读采集 · 写操作需授权</p>
    </aside>

    <main class="lgdocker-main">
      <div v-if="locked" class="lgdocker-lockbar" role="status">
        <LgIcon name="lock" :size="13" />
        <span>当前服务器已锁定，仅可查看。解锁后可执行 Docker 写操作。</span>
        <button type="button" class="lgdocker-btn is-small" @click="openPermitPanel()">解锁</button>
      </div>
      <OverviewPage v-if="page === 'overview'" :server="server" />
      <ContainersPage v-else-if="page === 'containers'" :server="server" />
      <ImagesPage v-else-if="page === 'images'" :server="server" />
      <RegistryPage v-else-if="page === 'registry'" :server="server" />
    </main>
  </div>
</template>
