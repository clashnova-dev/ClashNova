<script setup lang="ts">
  // "Edit info" dialog: changes a local subscription's name and source URL. Renaming takes effect immediately; changing the source URL re-fetches the subscription.
  import { nextTick, ref } from "vue";
  import { useApp } from "../composables/useApp";
  import BaseDialog from "./BaseDialog.vue";
  import BaseInput from "./ui/BaseInput.vue";
  import BaseButton from "./ui/BaseButton.vue";

  const { t, tx, saveExternalInfo } = useApp();
  const base = ref<InstanceType<typeof BaseDialog> | null>(null);
  const nameInput = ref<InstanceType<typeof BaseInput> | null>(null);
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
  function close() {
    base.value?.close();
  }

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
      <BaseInput
        ref="nameInput"
        v-model.trim="name"
        :label="t('app.subscriptionName')"
        :disabled="busy"
        :placeholder="t('app.forExampleEveryday')"
        maxlength="80"
        required
      />
      <BaseInput
        v-model.trim="url"
        :label="t('app.subscriptionUrl')"
        :disabled="busy"
        type="url"
        placeholder="https://example.com/subscribe"
      />
      <p class="hint">{{ t("subscriptions.editInfoHint") }}</p>
      <p v-if="error" class="form-error" role="alert">{{ tx(error) }}</p>
      <div class="dialog-actions">
        <BaseButton :disabled="busy" @click="close">{{ t("common.cancel") }}</BaseButton>
        <BaseButton variant="primary" type="submit" :disabled="busy || !name">
          {{ busy ? t("connections.generating") : t("lan.save") }}
        </BaseButton>
      </div>
    </form>
  </BaseDialog>
</template>
