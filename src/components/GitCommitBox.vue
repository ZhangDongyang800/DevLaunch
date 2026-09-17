<script setup lang="ts">
import { computed, ref } from 'vue'
import { busy, commit, statuses } from '../gitStore'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{ notify: [msg: string, kind?: 'ok' | 'err'] }>()

const message = ref('')
const amend = ref(false)

const status = computed(() => statuses.value[props.projectId])
const stagedCount = computed(() => status.value?.staged ?? 0)
const blocked = computed(() => !!status.value?.operation)
const canCommit = computed(
  () => !busy.value && !blocked.value && (amend.value || stagedCount.value > 0) && message.value.trim().length > 0,
)

async function doCommit() {
  if (!canCommit.value) return
  try {
    await commit(props.projectId, message.value, amend.value)
    message.value = ''
    amend.value = false
    emit('notify', '已提交')
  } catch (e) {
    emit('notify', `${e}`, 'err')
  }
}
</script>

<template>
  <div class="commit-box">
    <textarea
      v-model="message"
      class="commit-message"
      rows="3"
      :disabled="blocked"
      placeholder="提交信息…（Ctrl+Enter 提交）"
      spellcheck="false"
      @keydown.ctrl.enter.prevent="doCommit"
    />
    <div class="commit-actions">
      <label class="commit-amend"><input type="checkbox" v-model="amend" /> amend</label>
      <span class="commit-staged mono">已暂存 {{ stagedCount }} 个文件</span>
      <span class="v-spacer" />
      <span v-if="blocked" class="commit-busy">存在未完成的 {{ status?.operation }}，请先在终端处理</span>
      <span v-else-if="busy" class="commit-busy">处理中…</span>
      <button class="primary" :disabled="!canCommit" @click="doCommit">Commit</button>
    </div>
  </div>
</template>
