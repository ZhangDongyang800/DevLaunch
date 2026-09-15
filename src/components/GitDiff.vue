<script setup lang="ts">
withDefaults(
  defineProps<{
    stat?: string
    text?: string
    truncated?: boolean
    untracked?: boolean
    loading?: boolean
  }>(),
  { stat: '', text: '', truncated: false, untracked: false, loading: false },
)
</script>

<template>
  <div class="gc-body git-diff-body">
    <div v-if="loading" class="gc-empty">加载中…</div>
    <template v-else-if="text">
      <pre v-if="untracked" class="git-patch-tag">新文件内容</pre>
      <pre class="git-patch mono">{{ stat }}{{ text }}</pre>
      <div v-if="truncated" class="gt-truncated">内容过大，未完整渲染</div>
    </template>
    <div v-else class="gc-empty">选择一条提交或文件查看差异</div>
  </div>
</template>
