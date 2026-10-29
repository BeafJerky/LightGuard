<script setup lang="ts">
import type { Server } from '../types';

defineProps<{ server: Server }>();
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>镜像仓库</h2>
      </div>
    </header>
    <div class="lgdocker-empty">正在读取仓库配置…</div>
  </section>
</template>
