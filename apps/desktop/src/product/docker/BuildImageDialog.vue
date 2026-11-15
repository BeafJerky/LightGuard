<script setup lang="ts">
import { reactive, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import { toSshError } from '../ssh';
import { dockerApi } from './api';

const emit = defineEmits<{
  close: [];
  build: [payload: { contextPath: string; tag: string; dockerfile: string }];
}>();

const error = ref('');
const picking = ref(false);
const form = reactive({
  contextPath: '',
  tag: '',
  dockerfile: 'Dockerfile',
});

async function pickContext(): Promise<void> {
  if (picking.value) return;
  picking.value = true;
  try {
    const path = await dockerApi.pickDirectory();
    if (path) form.contextPath = path;
  } catch (caught) {
    error.value = toSshError(caught).message;
  } finally {
    picking.value = false;
  }
}

function submit(): void {
  const contextPath = form.contextPath.trim();
  const tag = form.tag.trim();
  const dockerfile = form.dockerfile.trim() || 'Dockerfile';
  if (!contextPath) {
    error.value = '请选择构建上下文目录';
    return;
  }
  if (!tag) {
    error.value = '请填写镜像标签';
    return;
  }
  emit('build', { contextPath, tag, dockerfile });
}
</script>

<template>
  <div class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="构建镜像">
    <div class="lgdocker-modal__mask" @click="emit('close')" />
    <section class="lgdocker-modal__panel">
      <header class="lgdocker-modal__head">
        <div>
          <strong>构建镜像</strong>
          <span>docker build -t &lt;tag&gt; -</span>
        </div>
        <button type="button" class="lgdocker-btn" @click="emit('close')">取消</button>
      </header>
      <div class="lgdocker-form">
        <label>
          <span>构建上下文</span>
          <div class="lgdocker-field">
            <input
              v-model="form.contextPath"
              placeholder="选择本地目录"
              aria-label="构建上下文"
              readonly
            />
            <button
              type="button"
              class="lgdocker-btn is-small"
              :disabled="picking"
              @click="pickContext()"
            >
              {{ picking ? '选择中…' : '选择' }}
            </button>
          </div>
        </label>
        <label>
          <span>镜像标签</span>
          <input v-model="form.tag" placeholder="my-app:1.0" aria-label="镜像标签" />
        </label>
        <label>
          <span>Dockerfile（相对上下文）</span>
          <input v-model="form.dockerfile" placeholder="Dockerfile" aria-label="Dockerfile" />
        </label>
        <p v-if="error" class="lgdocker-error">{{ error }}</p>
        <p class="lgdocker-muted">
          上下文会打包为 tar.gz 上传到服务器并立即删除；构建命令不会进入服务器历史。
        </p>
      </div>
      <footer class="lgdocker-modal__foot">
        <button type="button" class="lgdocker-btn" @click="emit('close')">取消</button>
        <button type="button" class="lgdocker-btn is-primary" @click="submit()">
          <LgIcon name="play" :size="13" />
          预览并构建
        </button>
      </footer>
    </section>
  </div>
</template>