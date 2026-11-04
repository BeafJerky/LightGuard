import { ref } from 'vue';
import type { Route, RouteName, SettingsTab } from './types';

// 控制台页面的中文标题唯一来源；`desktop` 由服务器名单独拼装，不在此表中。
export const ROUTE_TITLES: Record<Exclude<RouteName, 'desktop'>, string> = {
  overview: '服务器总览',
  alerts: '告警中心',
  connections: '连接记录',
  settings: '设置',
};

export function routeTitle(name: Exclude<RouteName, 'desktop'>): string {
  return ROUTE_TITLES[name];
}

const SETTINGS_TABS: ReadonlyArray<SettingsTab> = [
  'appearance',
  'general',
  'security',
  'alerts',
  'data',
];

function parseSettingsTab(value: string | undefined): SettingsTab | undefined {
  return SETTINGS_TABS.find((tab) => tab === value);
}

function parseHash(hash: string): Route {
  const clean = hash.replace(/^#\/?/, '').replace(/\/+$/, '');
  const parts = clean
    .split('/')
    .filter(Boolean)
    .map((part) => {
      try {
        return decodeURIComponent(part);
      } catch {
        return part;
      }
    });
  if (parts[0] === 'connections') return { name: 'connections' };
  if (parts[0] === 'alerts') {
    return { name: 'alerts', tab: parts[1] === 'audit' ? 'audit' : 'alerts' };
  }
  if (parts[0] === 'settings') {
    const tab = parseSettingsTab(parts[1]);
    return tab ? { name: 'settings', tab } : { name: 'settings' };
  }
  if (parts[0] === 'desktop' && parts[1]) return { name: 'desktop', id: parts[1] };
  return { name: 'overview' };
}

export function routeToHash(route: Route) {
  if (route.name === 'desktop' && route.id) return `#/desktop/${encodeURIComponent(route.id)}`;
  if (route.name === 'alerts') return route.tab === 'audit' ? '#/alerts/audit' : '#/alerts';
  if (route.name === 'settings' && route.tab) return `#/settings/${route.tab}`;
  return `#/${route.name}`;
}

export const route = ref<Route>(parseHash(window.location.hash));

export function navigate(next: Route, replace = false) {
  route.value = next;
  const hash = routeToHash(next);
  if (window.location.hash === hash) return;
  if (replace) {
    window.history.replaceState(null, '', hash);
  } else {
    window.location.hash = hash.slice(1);
  }
}

export function navigateTo(name: Exclude<Route['name'], 'desktop'>, replace = false) {
  navigate({ name }, replace);
}

export function openServerDesktop(id: string) {
  navigate({ name: 'desktop', id });
}

export function openAudit() {
  navigate({ name: 'alerts', tab: 'audit' });
}

let listening = false;
let previousName = route.value.name;

function onHashChange() {
  const next = parseHash(window.location.hash);
  if (next.name !== previousName) {
    previousName = next.name;
    window.scrollTo({ top: 0, behavior: 'instant' as ScrollBehavior });
  }
  route.value = next;
}

export function startRouter() {
  if (listening) return () => {};
  listening = true;
  window.addEventListener('hashchange', onHashChange);
  if (!window.location.hash) window.history.replaceState(null, '', routeToHash(route.value));
  return () => {
    listening = false;
    window.removeEventListener('hashchange', onHashChange);
  };
}
