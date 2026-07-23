# AVDL - 影片下載工具

Language: [简体中文](../README.md) | **繁體中文** | [English](README.en.md) | [日本語](README.ja.md)

基於 Tauri v2、React 與 Rust 構建的桌面影片下載與瀏覽工具，支援 JableTV、MissAV 與 SupJav。

![淺色模式](../docs/images/preview01.png)

![深色模式](../docs/images/preview02.png)

---

## 功能特性

### 下載與檔案處理
- **M3U8 切片下載與解密**：支援 TS 分片多執行緒並發下載、AES-128-CBC 自動解密與斷點續傳。
- **Faststart 播放優化**：合併 MP4 時追加 `-movflags +faststart` 參數，優化 keyframe 索引表位置，使產生的 MP4 支援拖曳進度條即時播放。
- **Streamtape 流式下載**：解密 Streamtape 混淆腳本，採用流式增量寫入磁碟。

### 站點適配與防爬處理
- **Cloudflare 驗證與狀態感知**：內建驗證視窗以獲取和管理 `cf_clearance` 憑證；當請求傳回 `403 Forbidden` 時自動清除失效憑證並提示重新驗證。
- **MissAV 鏡像自動切換**：主功能變數名稱連接失敗時，自動探測並切換至可用鏡像功能變數名稱。
- **Referer 防盜鏈處理**：請求資源時自動附加正確的 `Referer` 標頭，解決預覽與切片下載被攔截的問題。
- **SupJav 多源處理**：支援 `TV` / `FST` / `VOE` / `ST` 多個伺服器檢測與自動故障轉移；自動剝離切片中偽裝的 PNG 圖片標頭資料。
- **預告片自動過濾**：解析 M3U8 總時長，自動跳過低於 600 秒的短預覽影片並切換至正片資料源。

### 介面與互動
- **響應式網格佈局**：採用 CSS Grid `auto-fill`（最小寬度 240px），根據視窗尺寸自動調整列數，避免大螢幕封面拉伸模糊和小螢幕文字受擠壓。
- **卡片懸停預覽**：懸停卡片時載入影片預覽片段並產生記憶體 Blob URL 播放。
- **批量任務控制**：下載佇列提供雙行緊湊佈局與 Hover 工具提示，支援全選、多選以及批量開始、暫停和取消操作。
- **磁碟儲存分析**：獲取目標磁碟區的總容量、剩餘可用空間及下載檔案佔用比例。
- **多語言支援**：支援繁體中文、簡體中文、英文與日文介面，支援多語言影片標題。

---

## 技術棧

- **桌面框架**：Tauri v2
- **前端框架**：React 19, TypeScript, Vite
- **UI 與樣式**：Tailwind CSS, daisyUI, Lucide React
- **狀態管理**：Zustand
- **後端**：Rust (Edition 2021)
- **網路與解密**：`reqwest` / `wreq`, `scraper`, `aes` / `cbc`
- **影片合併**：FFmpeg（需要系統 PATH 中包含 `ffmpeg`）

---

## 運行與構建

### 依賴要求
- Node.js (v18+)
- Rust (1.75+)
- FFmpeg

### 開發與打包

1. **安裝依賴**
   ```bash
   npm install
   ```

2. **啟動開發服務**
   ```bash
   npm run tauri dev
   ```

3. **構建可執行檔**
   ```bash
   npm run tauri build
   ```

---

## 🍎 macOS 使用說明與常見問題

在運行打包後的 `.app` 或二進位檔案時請注意，軟體為開源未簽名應用，首次運行前如遇權限攔截，可在終端機執行命令解除 macOS 隔離限制：

```bash
# 解除應用包隔離
xattr -cr /Applications/avdl.app

# 解除單檔案二進位隔離
xattr -cr ./avdl
```

---

## 免責聲明

本專案僅供個人學習與技術研究使用。
