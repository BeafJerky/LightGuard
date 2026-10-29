import { ref } from 'vue';
import type { Route } from './types';

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
  if (parts[0] === 'settings') return { name: 'settings' };
  if (parts[0] === 'desktop' && parts[1]) return { name: 'desktop', id: parts[1] };
  return { name: 'overview' };
}

export function routeToHash(route: Route) {
  if (route.name === 'desktop' && route.id) return `#/desktop/${encodeURIComponent(route.id)}`;
  if (route.name === 'alerts') return route.tab === 'audit' ? '#/alerts/audit' : '#/alerts';
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
