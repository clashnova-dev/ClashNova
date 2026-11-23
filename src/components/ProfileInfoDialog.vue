<script setup lang="ts">
// 「编辑信息」弹窗:修改本地订阅的名称与源地址。改名即时生效;更换源地址会重新拉取订阅。
import { nextTick, ref } from "vue";
import { useApp } from "../composables/useApp";
import BaseDialog from "./BaseDialog.vue";

const { t, tx, saveExternalInfo } = useApp();
const base = ref<InstanceType<typeof BaseDialog> | null>(null);
const nameInput = ref<HTMLInputElement | null>(null);
const id = ref("");
const name = ref("");
const url = ref("");
const error = ref("");
const busy = ref(false);

function open(profile: { id: string; name: string; url: string }) {
  id.value = profile.id;
  name.value = profile.name;
  url.value = profile.url;
  error.value = "";
  base.value?.open();
  nextTick(() => nameInput.value?.focus());
}
function close() { base.value?.close(); }

async function save() {
  if (busy.value) return;
  error.value = "";
  busy.value = true;
  try {
    await saveExternalInfo(id.value, name.value, url.value);
    base.value?.close();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

defineExpose({ open });
</script>

<template>
  <BaseDialog ref="base" size="sm" :title="t('subscriptions.editInfoTitle')" @close="error = ''">
    <form class="profile-form" @submit.prevent="save">
      <label>{{ t('app.subscriptionName') }}<input ref="nameInput" v-model.trim="name" :disabled="busy" :placeholder="t('app.forExampleEveryday')" maxlength="80" required /></label>
      <label>{{ t('app.subscriptionUrl') }}<input v-model.trim="url" :disabled="busy" type="url" placeholder="https://example.com/subscribe" /></label>
      <p class="hint">{{ t('subscriptions.editInfoHint') }}</p>
      <p v-if="error" class="form-error" role="alert">{{ tx(error) }}</p>
      <div class="dialog-actions">
        <button type="button" :disabled="busy" @click="close">{{ t('common.cancel') }}</button>
        <button class="primary" type="submit" :disabled="busy || !name">{{ busy ? t('connections.generating') : t('lan.save') }}</button>
      </div>
    </form>
  </BaseDialog>
</template>
