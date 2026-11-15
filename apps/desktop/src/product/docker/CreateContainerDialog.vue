<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { dockerApi, type DockerImage } from './api';

const props = defineProps<{ server: Server }>();
const emit = defineEmits<{
  close: [];
  create: [payload: Record<string, unknown>];
}>();

const images = ref<DockerImage[]>([]);
const loading = ref(true);
const error = ref('');
const form = reactive({
  image: '',
  name: '',
  ports: '',
  env: '',
  restart: 'no',
  command: '',
});

const lines = (value: string) =>
  value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

async function loadImages(): Promise<void> {
  loading.value = true;
  try {
    images.value = await dockerApi.images(props.server.id);
  } catch (caught) {
    error.value = toSshError(caught).message;
  } finally {
    loading.value = false;
  }
}

function submit(): void {
  const image = form.image.trim();
  if (!image) {
    error.value = '请选择或输入镜像';
    return;
  }
  emit('create', {
    image,
    name: form.name.trim() || undefined,
    ports: lines(form.ports),
    env: lines(form.env),
    restart: form.restart,
    command: form.command.trim() || undefined,
  });
}

onMounted(loadImages);
</script>

<template>
  <div class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="新建容器">
    <div class="lgdocker-modal__mask" @click="emit('close')" />
    <section class="lgdocker-modal__panel">
      <header class="lgdocker-modal__head">
        <div>
          <strong>新建容器</strong>
          <span>docker run -d</span>
        </div>
        <button type="button" class="lgdocker-btn" @click="emit('close')">取消</button>
      </header>
      <div class="lgdocker-form">
        <label>
          <span>镜像</span>
          <input v-model="form.image" list="lg-local-images" placeholder="nginx:1.27" aria-label="镜像" />
          <datalist id="lg-local-images">
            <option v-for="image in images" :key="image.id" :value="`${image.repo}:${image.tags[0] ?? 'latest'}`" />
          </datalist>
        </label>
        <label>
          <span>容器名（可选）</span>
          <input v-model="form.name" placeholder="web-01" aria-label="容器名" />
        </label>
        <label>
          <span>端口映射（每行一条，如 8080:80）</span>
          <textarea v-model="form.ports" rows="2" aria-label="端口映射" />
        </label>
        <label>
          <span>环境变量（每行一条，如 TZ=Asia/Shanghai）</span>
          <textarea v-model="form.env" rows="2" aria-label="环境变量" />
        </label>
        <label>
          <span>重启策略</span>
          <select v-model="form.restart" aria-label="重启策略">
            <option value="no">不重启</option>
            <option value="always">always</option>
            <option value="unless-stopped">unless-stopped</option>
            <option value="on-failure">on-failure</option>
          </select>
        </label>
        <label>
          <span>启动命令（可选，按空格拆分，支持引号）</span>
          <input v-model="form.command" placeholder="/bin/sh -c ..." aria-label="启动命令" />
        </label>
        <p v-if="error" class="lgdocker-error">{{ error }}</p>
        <p class="lgdocker-muted">环境变量值不会写入审计；镜像必须已存在于本地。</p>
      </div>
      <footer class="lgdocker-modal__foot">
        <button type="button" class="lgdocker-btn" @click="emit('close')">取消</button>
        <button type="button" class="lgdocker-btn is-primary" :disabled="loading" @click="submit()">
          <LgIcon name="play" :size="13" />
          预览并创建
        </button>
      </footer>
    </section>
  </div>
</template>