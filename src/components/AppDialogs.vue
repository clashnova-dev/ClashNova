<template>
  <dialog
    ref="profileDialog"
    class="subscription-dialog"
    @close="resetProfileDialog"
    aria-labelledby="add-profile-title"
    @cancel="busy && $event.preventDefault()"
    @click="!busy && $event.target === profileDialog && closeProfileDialog()"
  >
    <div class="dialog-heading">
      <h2 id="add-profile-title">{{ t("subscriptions.add") }}</h2>
      <BaseButton
        variant="icon"
        :disabled="busy"
        :aria-label="t('app.closeAddSubscription')"
        @click="closeProfileDialog"
      >
        <X :size="20" />
      </BaseButton>
    </div>
    <p class="dialog-description">{{ t("app.addASubscriptionUrlOr") }}</p>
    <form class="profile-form" @submit.prevent="addExternal">
      <BaseInput
        ref="profileNameInput"
        v-model.trim="newProfileName"
        :label="t('app.subscriptionName')"
        :disabled="busy"
        :placeholder="t('app.forExampleEveryday')"
        maxlength="80"
        required
      />
      <BaseInput
        v-model.trim="newProfileUrl"
        :label="t('app.subscriptionUrl')"
        :disabled="busy || !!selectedFile"
        type="url"
        placeholder="https://example.com/subscribe"
      />
      <div class="file-import">
        <span>{{ t("app.orImportALocalFile") }}</span><label class="file-picker"
          ><Plus :size="16" />{{ selectedFile?.name || t("app.chooseFile")
          }}<input
            :disabled="busy"
            type="file"
            accept=".json,.yaml,.yml,.txt,application/json,text/yaml,text/plain"
            @change="chooseFile" /></label
        ><BaseButton v-if="selectedFile" variant="link" @click="selectedFile = null">
          {{ t("app.useAUrlInstead") }}
        </BaseButton>
      </div>
      <p class="hint">{{ t("app.supportsSingBoxJsonClash") }}</p>
      <p v-if="profileError" class="form-error" role="alert">{{ tx(profileError) }}</p>
      <div class="dialog-actions">
        <BaseButton :disabled="busy" @click="closeProfileDialog"> {{ t("common.cancel") }}</BaseButton><BaseButton
          variant="primary"
          :disabled="busy"
          type="submit"
        >
          {{ busy ? t("app.importing") : t("app.importUse") }}
        </BaseButton>
      </div>
    </form>
  </dialog>
  <dialog
    ref="importDialog"
    class="subscription-dialog link-confirm"
    aria-labelledby="import-link-title"
    @cancel="busy && $event.preventDefault()"
    @close="dismissImport"
    @click="!busy && $event.target === importDialog && dismissImport()"
  >
    <div class="dialog-heading">
      <h2 id="import-link-title">{{ t("deepLink.importTitle") }}</h2>
      <BaseButton variant="icon" :disabled="busy" :aria-label="t('deepLink.close')" @click="dismissImport">
        <X :size="20" />
      </BaseButton>
    </div>
    <p class="dialog-description">
      {{ importTarget ? t("deepLink.importUrlBody") : t("deepLink.importBody") }}
    </p>
    <div v-if="importTarget" class="import-target">
      <strong v-if="importTarget.name">{{ importTarget.name }}</strong><span>{{ importTarget.host }}</span>
    </div>
    <p v-if="importError" class="form-error" role="alert">{{ tx(importError) }}</p>
    <div class="dialog-actions">
      <BaseButton :disabled="busy" @click="dismissImport"> {{ t("common.cancel") }}</BaseButton><BaseButton
        variant="primary"
        :disabled="busy"
        @click="confirmImport"
      >
        {{ busy ? t("app.importing") : t("deepLink.confirmImport") }}
      </BaseButton>
    </div>
  </dialog>
</template>
<script setup lang="ts">
  import { X, Plus } from "@lucide/vue";
  import { useApp } from "../composables/useApp";
  import BaseButton from "./ui/BaseButton.vue";
  import BaseInput from "./ui/BaseInput.vue";

  const {
    profileDialog,
    resetProfileDialog,
    busy,
    closeProfileDialog,
    t,
    tx,
    addExternal,
    profileNameInput,
    newProfileName,
    newProfileUrl,
    selectedFile,
    chooseFile,
    profileError,
    importDialog,
    dismissImport,
    importTarget,
    importError,
    confirmImport,
  } = useApp();
</script>
