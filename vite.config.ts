import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  publicDir: "public",
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
    // The workbench, the 独立选点列表 window, the 超级鹰眼 window, 批量分析进度表 and the
    // 浮动主棋盘 / 浮动小棋盘 windows.
    rollupOptions: {
      input: {
        main: resolve(__dirname, "ui/index.html"),
        suggestions: resolve(__dirname, "ui/suggestions.html"),
        hawkeye: resolve(__dirname, "ui/hawkeye.html"),
        batch: resolve(__dirname, "ui/batch.html"),
        board: resolve(__dirname, "ui/board.html"),
      },
    },
  },
});
