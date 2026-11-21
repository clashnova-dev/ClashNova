<template><Transition name="notice"><div v-if="currentNotice" class="app-notice" :class="currentNotice.kind || 'info'" :role="currentNotice.kind === 'error' ? 'alert' : 'status'"><component :is="currentNotice.kind === 'error' ? CircleAlert : currentNotice.kind === 'success' ? CircleCheck : Info" :size="19" /><div><strong v-if="currentNotice.title">{{ currentNotice.title }}</strong><p>{{ currentNotice.body }}</p></div><button @click="dismissNotice" :aria-label="t('notifications.dismissNotification')"><X :size="16" /></button></div></Transition></template>
<script setup lang="ts">
import {CircleAlert,CircleCheck,Info,X} from '@lucide/vue';
import {currentNotice,dismissNotice} from '../lib/notifications';
import {translator, type Locale} from '../i18n';
const props=defineProps<{locale:Locale}>();const { t, tx } = translator(() => props.locale);
</script>
<style scoped>
.app-notice{position:fixed;z-index:90;right:24px;bottom:52px;max-width:min(440px,calc(100vw - 48px));display:flex;align-items:flex-start;gap:10px;padding:14px 16px;border:1px solid var(--border);border-radius:12px;background:var(--bg);color:var(--strong);box-shadow:0 8px 28px #14243b22;font-size:13px;line-height:1.6}.app-notice>svg{flex:none;margin-top:2px}.app-notice>div{min-width:0}.app-notice p{margin:0;overflow-wrap:anywhere}.app-notice strong{display:block;margin-bottom:2px}.app-notice.error>svg{color:var(--danger)}.app-notice.success>svg{color:var(--success)}.app-notice button{border:0;background:transparent;min-height:24px;min-width:24px;padding:2px;color:var(--muted)}.notice-enter-active,.notice-leave-active{transition:opacity .15s,transform .15s}.notice-enter-from,.notice-leave-to{opacity:0;transform:translateY(8px)}@media(prefers-reduced-motion:reduce){.notice-enter-active,.notice-leave-active{transition:none}}
</style>
