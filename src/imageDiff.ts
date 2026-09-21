/**
 * 图片版本比较：两侧 data URL → 差异图。
 *
 * 为什么放在前端：解码是浏览器自带的能力（Chromium/WebView2 的 image 解码器），
 * Rust 侧只负责把两侧**原样字节**取回来；引入图片库去做解码是白多一份依赖。
 *
 * 内存边界（重要）：后端只保证"压缩后 ≤4MB 且解码后 ≤4000 万像素"，
 * 但逐像素比较同时要持有 2 份输入像素数组 + 1 份输出 ImageData，各是
 * `W×H×4` 字节——6000×6000 就是 430MB，足够让 WebView2 直接崩。
 * 所以这里**先等比降采样到 `MAX_DIFF_EDGE` 以内再比**：像素数组的规模被钉死，
 * 代价只是差异判定的粒度变粗（结果里如实标出 `scaled`）。
 */

export type PixelDiff =
  | {
      kind: 'diff'
      url: string
      changed: number
      total: number
      width: number
      height: number
      /** 比较是否在降采样后的尺寸上进行 */
      scaled: boolean
      sourceWidth: number
      sourceHeight: number
    }
  | { kind: 'sizeMismatch'; from: string; to: string }
  | { kind: 'error'; message: string }

/**
 * 逐像素比较时单边的最大边长。
 * 2048 → 最多 2048×2048×4×3 ≈ 50MB，是 WebView 能轻松承受的量级。
 */
export const MAX_DIFF_EDGE = 2048

function loadImg(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const im = new Image()
    im.onload = () => resolve(im)
    im.onerror = () => reject(new Error('无法解码'))
    im.src = url
  })
}

export function formatSize(w: number, h: number) {
  return `${w}×${h}`
}

/** 单边尺寸（一次 onload，不是轮询）；解不开返回空串。 */
export function measureSize(url: string): Promise<string> {
  return loadImg(url)
    .then((im) => formatSize(im.naturalWidth, im.naturalHeight))
    .catch(() => '')
}

/** 按目标尺寸取像素；`drawImage` 的目标尺寸与自然尺寸不同时就在这里降采样。 */
function rgba(img: HTMLImageElement, w: number, h: number): Uint8ClampedArray {
  const cv = document.createElement('canvas')
  cv.width = w
  cv.height = h
  const ctx = cv.getContext('2d')
  if (!ctx) throw new Error('无法创建画布')
  // 必须先 drawImage 再 getImageData，否则读回全 0。
  ctx.drawImage(img, 0, 0, w, h)
  // 源被视为跨域时在这里抛 SecurityError
  return ctx.getImageData(0, 0, w, h).data
}

/**
 * 逐像素差异：底图 = 新图压暗灰度，不同像素标红。
 *
 * 比的是**解码后的像素**，不是字节——同一张图重新压缩后字节全变而像素不必变，
 * 所以这里的 changed 与文件字节差是两回事，两个数都要给用户。
 * 两侧都完全透明的像素不可见，算作相同（PNG 优化后的透明像素常带任意颜色）。
 */
export async function pixelDiff(oldUrl: string, newUrl: string): Promise<PixelDiff> {
  let ia: HTMLImageElement | undefined
  let ib: HTMLImageElement | undefined
  try {
    ;[ia, ib] = await Promise.all([loadImg(oldUrl), loadImg(newUrl)])
    if (ia.naturalWidth !== ib.naturalWidth || ia.naturalHeight !== ib.naturalHeight) {
      return {
        kind: 'sizeMismatch',
        from: formatSize(ia.naturalWidth, ia.naturalHeight),
        to: formatSize(ib.naturalWidth, ib.naturalHeight),
      }
    }
    const sourceWidth = ib.naturalWidth
    const sourceHeight = ib.naturalHeight
    const scale = Math.min(1, MAX_DIFF_EDGE / Math.max(sourceWidth, sourceHeight))
    const width = Math.max(1, Math.round(sourceWidth * scale))
    const height = Math.max(1, Math.round(sourceHeight * scale))

    const pa = rgba(ia, width, height)
    const pb = rgba(ib, width, height)
    const cv = document.createElement('canvas')
    cv.width = width
    cv.height = height
    const ctx = cv.getContext('2d')
    if (!ctx) throw new Error('无法创建画布')
    const out = ctx.createImageData(width, height)
    let changed = 0
    for (let i = 0; i < out.data.length; i += 4) {
      const invisible = pa[i + 3] === 0 && pb[i + 3] === 0
      const same =
        invisible ||
        (pa[i] === pb[i] && pa[i + 1] === pb[i + 1] && pa[i + 2] === pb[i + 2] && pa[i + 3] === pb[i + 3])
      if (same) {
        const y = Math.round((pb[i] * 0.299 + pb[i + 1] * 0.587 + pb[i + 2] * 0.114) * 0.5)
        out.data[i] = y
        out.data[i + 1] = y
        out.data[i + 2] = y
      } else {
        changed++
        out.data[i] = 255
        out.data[i + 1] = 64
        out.data[i + 2] = 60
      }
      out.data[i + 3] = 255
    }
    ctx.putImageData(out, 0, 0)
    return {
      kind: 'diff',
      url: cv.toDataURL('image/png'),
      changed,
      total: width * height,
      width,
      height,
      scaled: scale < 1,
      sourceWidth,
      sourceHeight,
    }
  } catch (e) {
    return { kind: 'error', message: e instanceof Error ? e.message : `${e}` }
  } finally {
    // 及时丢掉解码后的位图引用，让 GC 能回收（大图每份都是百 MB 量级）。
    if (ia) ia.src = ''
    if (ib) ib.src = ''
  }
}
