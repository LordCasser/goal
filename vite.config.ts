import { defineConfig, type UserConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// vitest 自带一份 vite 类型（rollup 系），与工作区 vite 8 的 rolldown 插件
// 类型不兼容，无法直接从 vitest/config 导入 defineConfig；test 字段在
// 这里用本地类型补充，vitest 运行时会照常读取。
type ViteConfigWithTests = UserConfig & {
  test?: { environment?: "node" | "jsdom" | "happy-dom"; globals?: boolean };
};

const config: ViteConfigWithTests = {
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  // Tauri 期望固定端口；占用则直接失败，避免 devUrl 与真实端口不一致。
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "safari16",
    sourcemap: true,
  },
  // 组件测试跑在 jsdom；globals 供 @testing-library 自动 cleanup。
  test: {
    environment: "jsdom",
    globals: true,
  },
};

export default defineConfig(config);
