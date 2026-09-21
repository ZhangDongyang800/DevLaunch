import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// `TAURI_DEV_HOST` 由 `tauri dev --host <ip>` 注入到 Vite 进程的环境变量里。
// 不引 `@types/node`：全仓只有这一处需要 process，用一个窄断言即可——为它引入
// 整包 Node 类型会让 `process` 看起来到处可用（而这个前端并不跑在 Node 里）。
const host = (
  globalThis as { process?: { env?: Record<string, string | undefined> } }
).process?.env?.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [vue()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
