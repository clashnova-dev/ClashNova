# pages/ —— 按页面组织

每个页面一个子目录,**页面与样式同目录**:

```
pages/<page>/
  index.vue       # 页面组件
  index.scss      # 该页样式,<style scoped lang="scss" src="./index.scss">
  *.ts           # 该页专属逻辑 / 数据(如 help-content.ts)
  components/     # 仅该页用到的组件(可选)
```

`App.vue` 以 `import X from './pages/<page>/index.vue'` 挂载。

## 状态(Pinia)
跨功能共享状态放 `src/stores/`(`core` / `i18n` / `logs` …),页面用 `storeToRefs`
解构 state/getter;从 `composables/useApp.ts` 自底向上逐步抽取。

## 迁移现状
已迁移:`help`、`login`。未迁移的仍是 `pages/*Page.vue`(过渡期并存),逐页迁入子目录。
