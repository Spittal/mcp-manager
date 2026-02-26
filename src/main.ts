import { createApp } from 'vue';
import { createPinia } from 'pinia';
import { createRouter, createWebHistory } from 'vue-router';
import VNetworkGraph from 'v-network-graph';
import 'v-network-graph/lib/style.css';
import App from './App.vue';
import './assets/main.css';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/servers',
    },
    {
      path: '/servers',
      name: 'servers',
      component: () => import('./views/ServerDetailView.vue'),
    },
    {
      path: '/servers/:id',
      name: 'server-detail',
      component: () => import('./views/ServerDetailView.vue'),
    },
    {
      path: '/add',
      name: 'add-server',
      component: () => import('./views/AddServerView.vue'),
    },
    {
      path: '/edit/:id',
      name: 'edit-server',
      component: () => import('./views/EditServerView.vue'),
    },
    {
      path: '/skills',
      name: 'skills',
      component: () => import('./views/SkillDetailView.vue'),
    },
    {
      path: '/skills/add',
      name: 'add-skill',
      component: () => import('./views/AddSkillView.vue'),
    },
    {
      path: '/skills/:id(.+)',
      name: 'skill-detail',
      component: () => import('./views/SkillDetailView.vue'),
    },
    {
      path: '/plugins',
      name: 'plugins',
      component: () => import('./views/PluginDetailView.vue'),
    },
    {
      path: '/plugins/add',
      name: 'add-plugin',
      component: () => import('./views/AddPluginView.vue'),
    },
    {
      path: '/plugins/:id(.+)',
      name: 'plugin-detail',
      component: () => import('./views/PluginDetailView.vue'),
    },
    {
      path: '/memories',
      name: 'memories',
      component: () => import('./views/MemoryBrowserView.vue'),
    },
    {
      path: '/memory-graph',
      name: 'memory-graph',
      component: () => import('./views/MemoryGraphView.vue'),
    },
    {
      path: '/memory-data',
      name: 'memory-data',
      component: () => import('./views/MemoryDataView.vue'),
    },
    {
      path: '/status',
      name: 'status',
      component: () => import('./views/SystemStatusView.vue'),
    },
    {
      path: '/marketplace',
      redirect: '/add',
    },
    {
      path: '/marketplace/:id',
      name: 'marketplace-detail',
      component: () => import('./views/MarketplaceDetailView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('./views/SettingsView.vue'),
    },
  ],
});

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(VNetworkGraph);
app.mount('#app');
