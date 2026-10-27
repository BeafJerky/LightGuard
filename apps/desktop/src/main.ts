import { createApp } from 'vue';
import '@lightguard/lightui/styles.css';
import './styles/global.css';
import App from './App.vue';
import { bootstrapStore } from './product/store';
import { bootstrapRenderSettings } from './render/settings';

async function start() {
  try {
    await bootstrapStore();
    await bootstrapRenderSettings();
  } catch (error) {
    console.error('[lightguard] persistence bootstrap failed', error);
  }
  createApp(App).mount('#app');
}

void start();
