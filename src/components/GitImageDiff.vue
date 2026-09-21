<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { measureSize, pixelDiff } from '../imageDiff'
import type { BinaryPreview, BlobSide } from '../types'

const props = defineProps<{ preview: BinaryPreview | null; loading: boolean; error?: string }>()

type Mode = 'side' | 'onion' | 'diff'
const mode = ref<Mode>('side')
const onion = ref(50)
const diffUrl = ref('')
const diffStat = ref('')
const diffNote = ref('')
const computing = ref(false)
const oldDims = ref('')
const newDims = ref('')
const decodeFail = ref(false)
const diffScaleNote = ref('')
// 换文件后旧响应的守卫（与 diff 面板其余部分同一套做法）
let token = 0

const oldSide = computed(() => props.preview?.old ?? null)
const newSide = computed(() => props.preview?.new ?? null)
const hasBoth = computed(() => !!(oldSide.value?.dataUrl && newSide.value?.dataUrl))
const oneSided = computed(() => !!(oldSide.value ?? newSide.value) && !(oldSide.value && newSide.value))
/** 解码后像素数超限（与"文件本身超 4MB"分开说，原因不同、用户的处置也不同） */
const overPixels = computed(() => !!(oldSide.value?.overPixels || newSide.value?.overPixels))
const overBytes = computed(
  () => !!(oldSide.value?.tooBig && !oldSide.value.overPixels) || !!(newSide.value?.tooBig && !newSide.value.overPixels),
)

/** 后端从文件头读出的尺寸优先——不必为了显示尺寸去解码整张图。 */
function dimsOf(s: BlobSide | null): string {
  if (!s?.width || !s?.height) return ''
  return `${s.width}×${s.height}`
}

function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1048576).toFixed(2)} MB`
}

function sideLabel(s: BlobSide | null, dims: string) {
  if (!s) return '不存在'
  const fmt = s.mime ? s.mime.slice(s.mime.indexOf('/') + 1).toUpperCase() : '二进制'
  return [fmt, dims, fmtBytes(s.size)].filter(Boolean).join(' · ')
}

const metaLine = computed(() => {
  const delta =
    oldSide.value && newSide.value
      ? `（${newSide.value.size >= oldSide.value.size ? '+' : '−'}${fmtBytes(
          Math.abs(newSide.value.size - oldSide.value.size),
        )}）`
      : ''
  return `${sideLabel(oldSide.value, oldDims.value)} → ${sideLabel(newSide.value, newDims.value)}${delta}`
})

watch(
  () => props.preview,
  (p) => {
    const my = ++token
    mode.value = 'side'
    diffUrl.value = ''
    diffStat.value = ''
    diffNote.value = ''
    diffScaleNote.value = ''
    decodeFail.value = false
    computing.value = false
    onion.value = 50
    oldDims.value = dimsOf(p?.old ?? null)
    newDims.value = dimsOf(p?.new ?? null)
    // 后端已经用文件头量出尺寸了，只有它认不出格式时才需要解码去量（解码很贵）。
    if (!oldDims.value && p?.old?.dataUrl) {
      void measureSize(p.old.dataUrl).then((d) => my === token && (oldDims.value = d))
    }
    if (!newDims.value && p?.new?.dataUrl) {
      void measureSize(p.new.dataUrl).then((d) => my === token && (newDims.value = d))
    }
  },
  { immediate: true },
)

async function computeDiff() {
  const a = oldSide.value?.dataUrl
  const b = newSide.value?.dataUrl
  if (!a || !b || computing.value) return
  const my = token
  computing.value = true
  diffNote.value = ''
  diffScaleNote.value = ''
  const got = await pixelDiff(a, b)
  if (my !== token) return
  computing.value = false
  if (got.kind === 'diff') {
    diffUrl.value = got.url
    diffStat.value = `${got.changed.toLocaleString()} / ${got.total.toLocaleString()} 像素不同（${(
      (got.changed / got.total) *
      100
    ).toFixed(1)}%）`
    // 降采样会改变判定粒度，必须说出来，否则用户会以为"差异就这么几处"。
    diffScaleNote.value = got.scaled
      ? `原图 ${got.sourceWidth}×${got.sourceHeight} 较大，已按 ${got.width}×${got.height} 降采样比较（差异计数按降采样后的像素计）`
      : ''
  } else {
    diffUrl.value = ''
    diffNote.value =
      got.kind === 'sizeMismatch'
        ? `两版本尺寸不同（${got.from} → ${got.to}），像素无法一一对应；改用「并排」或「叠加」`
        : `像素比对不可用：${got.message}`
  }
}

function pick(m: Mode) {
  mode.value = m
  if (m === 'diff' && !diffUrl.value) void computeDiff()
}
</script>

<template>
  <div class="img-diff">
    <div v-if="loading" class="gc-empty">读取内容…</div>
    <div v-else-if="error" class="gc-empty">{{ error }}</div>
    <div v-else-if="!preview" class="gc-empty">无内容</div>
    <template v-else-if="preview.image">
      <div class="diff-toolbar mono">
        <button class="ghost" :class="{ on: mode === 'side' }" @click="pick('side')">并排</button>
        <button class="ghost" :class="{ on: mode === 'onion' }" :disabled="!hasBoth" @click="pick('onion')">叠加</button>
        <button class="ghost" :class="{ on: mode === 'diff' }" :disabled="!hasBoth" @click="pick('diff')">差异</button>
        <label v-if="mode === 'onion'" class="onion-ctl">
          新图
          <input v-model.number="onion" type="range" min="0" max="100" step="1" />
          {{ onion }}%
        </label>
        <span v-if="computing" class="img-computing">计算中…</span>
        <span class="v-spacer" />
        <span class="img-meta">{{ metaLine }}</span>
      </div>

      <div class="img-stage">
        <template v-if="mode === 'side'">
          <figure v-if="oldSide" class="img-cell">
            <figcaption>修改前</figcaption>
            <div class="img-frame">
              <img v-if="oldSide.dataUrl" :src="oldSide.dataUrl" alt="" @error="decodeFail = true" />
              <div v-else class="img-none">超出预览上限，未加载</div>
            </div>
          </figure>
          <figure v-if="newSide" class="img-cell">
            <figcaption>修改后</figcaption>
            <div class="img-frame">
              <img v-if="newSide.dataUrl" :src="newSide.dataUrl" alt="" @error="decodeFail = true" />
              <div v-else class="img-none">超出预览上限，未加载</div>
            </div>
          </figure>
        </template>

        <div v-else-if="mode === 'onion'" class="img-onion">
          <div class="img-frame">
            <img v-if="oldSide?.dataUrl" :src="oldSide.dataUrl" alt="" @error="decodeFail = true" />
            <img
              v-if="newSide?.dataUrl"
              class="onion-top"
              :src="newSide.dataUrl"
              alt=""
              :style="{ opacity: onion / 100 }"
              @error="decodeFail = true"
            />
          </div>
        </div>

        <figure v-else class="img-cell wide">
          <figcaption>{{ diffStat || '逐像素差异' }}</figcaption>
          <div class="img-frame">
            <img v-if="diffUrl" :src="diffUrl" alt="" @error="decodeFail = true" />
            <div v-else class="img-none">{{ computing ? '计算中…' : diffNote || '正在计算…' }}</div>
          </div>
        </figure>
      </div>

      <div v-if="oneSided" class="gt-truncated">只存在一侧（新增或删除），无比较对象</div>
      <div v-else-if="overPixels" class="gt-truncated">
        单侧解码后超过 4000 万像素，为避免界面卡死未加载内容（可用文件管理器打开查看）
      </div>
      <div v-else-if="overBytes" class="gt-truncated">单侧内容超过 4 MB 预览上限，未加载内容</div>
      <div v-if="diffScaleNote" class="gt-truncated">{{ diffScaleNote }}</div>
      <div v-if="decodeFail" class="gt-truncated">WebView 无法解码该图片，可用文件管理器打开查看</div>
    </template>
    <div v-else class="gc-empty">
      二进制文件 · {{ metaLine }}
      <div class="img-hint">
        仅 PNG / JPEG / GIF / WebP / BMP / ICO / AVIF 且单侧 ≤4 MB、≤4000 万像素按图片预览（以魔数判定，不看扩展名）
      </div>
    </div>
  </div>
</template>

<style scoped>
.img-diff {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
}
.img-stage {
  display: flex;
  flex: 1;
  min-height: 0;
  gap: var(--sp-4);
  align-items: flex-start;
  justify-content: center;
  padding: var(--sp-4);
  overflow: auto;
}
.img-cell {
  display: flex;
  min-width: 0;
  flex: 0 1 auto;
  flex-direction: column;
  gap: var(--sp-2);
  margin: 0;
}
.img-cell.wide {
  max-width: 92%;
}
.img-cell figcaption {
  color: var(--faint);
  font-family: var(--mono);
  font-size: 11px;
  text-align: center;
}
/* 透明像素需要棋盘底，否则改了 alpha 看起来什么都没发生 */
.img-frame {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border);
  padding: var(--sp-2);
  background-color: var(--panel);
  background-image:
    linear-gradient(45deg, var(--elevated) 25%, transparent 25%),
    linear-gradient(-45deg, var(--elevated) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, var(--elevated) 75%),
    linear-gradient(-45deg, transparent 75%, var(--elevated) 75%);
  background-position: 0 0, 0 8px, 8px -8px, -8px 0;
  background-size: 16px 16px;
}
.img-frame img {
  max-width: 100%;
  height: auto;
  /* 像素画放大时保持方块，不做平滑 */
  image-rendering: pixelated;
}
.img-onion {
  display: flex;
  align-items: flex-start;
  justify-content: center;
  width: 100%;
}
.img-onion .img-frame {
  position: relative;
  display: block;
}
.img-onion .onion-top {
  position: absolute;
  top: var(--sp-2);
  left: var(--sp-2);
}
.img-none {
  padding: var(--sp-4) var(--sp-6);
  color: var(--faint);
  font-size: 12px;
}
.img-meta {
  color: var(--muted);
  white-space: nowrap;
}
.img-computing {
  color: var(--faint);
}
.onion-ctl {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  color: var(--muted);
}
.onion-ctl input {
  width: 120px;
  accent-color: var(--signal);
}
.img-hint {
  margin-top: var(--sp-1);
  color: var(--faint);
  font-size: 11.5px;
}
</style>
