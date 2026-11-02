import { createApp } from 'vue';
import '@lightguard/lightui/styles.css';
import './styles/global.css';
import App from './App.vue';
import { startNotificationActions } from './product/alerts/notifications';
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
  // 只在应用挂载后注册一次：通知点击要能跳转，需要路由与告警 store 已就绪。
  void startNotificationActions();
}

void start();
