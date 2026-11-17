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
    // The workbench, the 独立选点列表 window, the 超级鹰眼 window, 批量分析进度表, the
    // 浮动主棋盘 / 浮动小棋盘 windows, 抓取死活题's full-screen overlay, the AI 解说 window,
    // 棋盘同步工具's float board, 野狐棋谱, 腾讯棋谱, 弈客直播中心, the 弈客 web window's toolbar
    // and 独立棋局控制面板.
    rollupOptions: {
      input: {
        main: resolve(__dirname, "ui/index.html"),
        suggestions: resolve(__dirname, "ui/suggestions.html"),
        hawkeye: resolve(__dirname, "ui/hawkeye.html"),
        batch: resolve(__dirname, "ui/batch.html"),
        board: resolve(__dirname, "ui/board.html"),
        capture: resolve(__dirname, "ui/capture.html"),
        commentary: resolve(__dirname, "ui/commentary.html"),
        floatboard: resolve(__dirname, "ui/floatboard.html"),
        foxkifu: resolve(__dirname, "ui/foxkifu.html"),
        tencentkifu: resolve(__dirname, "ui/tencentkifu.html"),
        yikelive: resolve(__dirname, "ui/yikelive.html"),
        yikebrowser: resolve(__dirname, "ui/yikebrowser.html"),
        control: resolve(__dirname, "ui/control.html"),
      },
    },
  },
});
