<script setup lang="ts">
// 「编辑规则」弹窗(规则页入口):结构化增删改当前外部订阅自带的 route.rules。
// 左侧表单(类型 + 内容 + 代理策略)添加/修改;右侧列出自定义规则(可过滤、编辑、删除)。
// 只处理单匹配简单规则;复杂规则原样保留。保存经 saveExternalRules 校验下发(仅替换 route.rules)。
import { computed, ref } from "vue";
import { Pencil, Trash2 } from "@lucide/vue";
import { useApp } from "../composables/useApp";
import BaseDialog from "./BaseDialog.vue";
import SelectMenu from "./ui/SelectMenu.vue";
import { RULE_POLICIES, RULE_TYPES, ruleCode, ruleToUi, uiToRule, validRuleValue, type RulePolicy, type UiRule } from "../lib/route-rules";

const { t, tx, saveExternalRules } = useApp();
const base = ref<InstanceType<typeof BaseDialog> | null>(null);
const id = ref("");
const rules = ref<UiRule[]>([]);
const preserved = ref<unknown[]>([]);
const filter = ref("");
const error = ref("");
const busy = ref(false);

const draftType = ref<string>("domain_suffix");
const draftValue = ref("");
const draftPolicy = ref<RulePolicy>("direct");
const editingId = ref<string | null>(null);
const contentInvalid = ref(false);

const typeOptions = computed(() => RULE_TYPES.map(r => ({ value: r.key as string, label: `${t(r.nameKey as any)} (${r.code})` })));
const policyOptions = computed(() => RULE_POLICIES.map(p => ({ value: p.value, label: `${t(p.nameKey as any)} (${p.code})` })));
const shown = computed(() => {
  const q = filter.value.trim().toLowerCase();
  return q ? rules.value.filter(r => r.value.toLowerCase().includes(q) || ruleCode(r.type).toLowerCase().includes(q)) : rules.value;
});

function open(profile: { id: string; raw: string }) {
  id.value = profile.id;
  error.value = "";
  resetForm();
  filter.value = "";
  try {
    const config = JSON.parse(profile.raw);
    const list: unknown[] = Array.isArray(config?.route?.rules) ? config.route.rules : [];
    const ui: UiRule[] = [];
    const keep: unknown[] = [];
    for (const rule of list) {
      const converted = ruleToUi(rule);
      if (converted) ui.push(converted); else keep.push(rule);
    }
    rules.value = ui;
    preserved.value = keep;
  } catch {
    rules.value = [];
    preserved.value = [];
    error.value = "error.subscriptionNotSingBox";
  }
  base.value?.open();
}

function resetForm() {
  draftType.value = "domain_suffix";
  draftValue.value = "";
  draftPolicy.value = "direct";
  editingId.value = null;
  contentInvalid.value = false;
}

function submitRule() {
  if (!validRuleValue(draftType.value, draftValue.value)) {
    contentInvalid.value = true;
    return;
  }
  const value = draftValue.value.trim();
  if (editingId.value) {
    rules.value = rules.value.map(r => (r.id === editingId.value ? { ...r, type: draftType.value, value, policy: draftPolicy.value } : r));
  } else {
    rules.value = [...rules.value, { id: crypto.randomUUID(), type: draftType.value, value, policy: draftPolicy.value }];
  }
  resetForm();
}

function editRule(rule: UiRule) {
  draftType.value = rule.type;
  draftValue.value = rule.value;
  draftPolicy.value = rule.policy;
  editingId.value = rule.id;
  contentInvalid.value = false;
}
function removeRule(ruleId: string) {
  rules.value = rules.value.filter(r => r.id !== ruleId);
  if (editingId.value === ruleId) resetForm();
}
function close() { base.value?.close(); }

async function save() {
  if (busy.value) return;
  error.value = "";
  busy.value = true;
  try {
    const out = [...rules.value.map(uiToRule), ...preserved.value];
    await saveExternalRules(id.value, JSON.stringify(out));
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
  <BaseDialog ref="base" size="lg" :title="t('rules.edit')" @close="error = ''">
    <div class="re-body">
      <p class="re-hint">{{ t('rules.editHint') }}</p>
      <form class="re-form" @submit.prevent="submitRule">
        <label class="re-field"><span>{{ t('rules.fieldType') }}</span><SelectMenu v-model="draftType" :label="t('rules.fieldType')" :options="typeOptions" /></label>
        <label class="re-field"><span>{{ t('rules.fieldContent') }}</span><input v-model="draftValue" class="re-input" :class="{ invalid: contentInvalid }" placeholder="example.com" @input="contentInvalid = false" /></label>
        <label class="re-field"><span>{{ t('rules.fieldPolicy') }}</span><SelectMenu v-model="draftPolicy" :label="t('rules.fieldPolicy')" :options="policyOptions" /></label>
        <p v-if="contentInvalid" class="re-invalid">{{ t('rules.contentInvalid') }}</p>
        <div class="re-form-actions">
          <button v-if="editingId" type="button" class="re-cancel-edit" @click="resetForm">{{ t('rules.cancelEdit') }}</button>
          <button type="submit" class="primary re-add">{{ editingId ? t('rules.updateRule') : t('rules.addRule') }}</button>
        </div>
      </form>
      <div class="re-list-wrap">
        <input v-model="filter" class="re-filter" type="text" :placeholder="t('rules.filter')" />
        <div class="re-list">
          <div v-for="rule in shown" :key="rule.id" class="re-row" :class="{ editing: rule.id === editingId }">
            <div class="re-row-meta"><span class="re-row-value" :title="rule.value">{{ rule.value }}</span><span class="re-row-badge">{{ ruleCode(rule.type) }}</span></div>
            <button type="button" class="icon-button re-row-btn" :aria-label="t('common.edit')" :title="t('common.edit')" @click="editRule(rule)"><Pencil :size="15" /></button>
            <button type="button" class="icon-button re-row-btn re-row-del" :aria-label="t('rules.deleteRule')" :title="t('rules.deleteRule')" @click="removeRule(rule.id)"><Trash2 :size="15" /></button>
          </div>
          <p v-if="!shown.length" class="re-empty">{{ t('rules.editEmpty') }}</p>
        </div>
      </div>
    </div>
    <p v-if="error" class="form-error re-error" role="alert">{{ tx(error) }}</p>
    <template #footer>
      <span class="re-count">{{ t('rules.count', { count: rules.length }) }}</span>
      <button type="button" :disabled="busy" @click="close">{{ t('common.cancel') }}</button>
      <button type="button" class="primary" :disabled="busy" @click="save">{{ busy ? t('connections.generating') : t('lan.save') }}</button>
    </template>
  </BaseDialog>
</template>

<style scoped>
.re-body { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 18px; }
.re-hint { grid-column: 1 / -1; margin: 0; color: var(--muted); font-size: 13px; line-height: 1.5; }
.re-form { display: flex; flex-direction: column; gap: 14px; }
.re-field { display: grid; gap: 7px; font-size: 13px; font-weight: 550; }
.re-field > span { color: var(--strong); }
.re-input { width: 100%; min-height: 40px; padding: 9px 12px; border: 1px solid var(--border); border-radius: 9px; background: var(--bg); color: var(--text); font-size: 14px; font-weight: 400; }
.re-input.invalid { border-color: var(--danger); }
.re-invalid { margin: 0; color: var(--danger); font-size: 12px; }
.re-form-actions { display: flex; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 2px; }
.re-cancel-edit { min-height: 36px; }
.re-add { min-height: 36px; padding: 0 18px; }
.re-list-wrap { display: flex; flex-direction: column; min-height: 0; }
.re-filter { flex: none; width: 100%; min-height: 38px; padding: 8px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg); color: var(--text); font-size: 14px; }
.re-list { flex: 1; min-height: 0; margin-top: 10px; overflow: auto; display: flex; flex-direction: column; gap: 8px; scrollbar-width: thin; scrollbar-color: var(--scrollbar) transparent; }
.re-row { display: flex; align-items: center; gap: 8px; padding: 9px 10px 9px 12px; border-radius: 10px; background: var(--surface); border: 1px solid transparent; }
.re-row.editing { border-color: var(--selected-border); background: var(--selected); }
.re-row-meta { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; }
.re-row-value { font-size: 14px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.re-row-badge { flex: none; padding: 1px 7px; border: 1px solid var(--border); border-radius: 6px; color: var(--muted); font-size: 11px; }
.re-row-btn { flex: none; width: 30px; min-height: 30px; padding: 7px; color: var(--muted); }
.re-row-del:hover { color: var(--danger); background: var(--danger-bg); }
.re-empty { margin: 24px 0; text-align: center; color: var(--muted); font-size: 13px; }
.re-error { flex: none; margin: 12px 0 0; }
.re-count { margin-right: auto; color: var(--muted); font-size: 13px; }
@media (max-width: 680px) { .re-body { grid-template-columns: 1fr; } }
</style>
