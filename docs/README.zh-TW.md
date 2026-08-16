# AVDL - 影片下載工具

Language: [简体中文](../README.md) | **繁體中文** | [English](README.en.md) | [日本語](README.ja.md)

Discord: [加入我們的 Discord 社群](https://discord.gg/GACc7HhHY)

> **極致微小 · 高能無界** — 基於 Tauri v2、React 和 Rust 構建的高性能桌面影片下載與瀏覽工具，套件體積僅 **約 10 MB**，極低資源佔用。支援 JableTV、MissAV 和 SupJav。

![淺色模式](../docs/images/preview01.png)

![深色模式](../docs/images/preview02.png)

---

## 功能特性

### 下載與檔案處理

- **M3U8 切片下載與解密**：支援 TS 分片多執行緒並發下載、AES-128-CBC 自動解密與斷點續傳。
- **Streamtape 流式下載**：解密 Streamtape 混淆腳本，採用流式增量寫入磁碟。

### 站點適配與防爬處理

- **Cloudflare 驗證與狀態感知**：內建驗證視窗以獲取和管理 Cloudflare 憑證，自動清除失效憑證並提示重新驗證。
- **智能代理感知與適配**：自動感知並匹配作業系統當前的 HTTP / SOCKS5 代理設定（無縫相容 Clash / Shadowsocks 等代理工具）。
- **MissAV 鏡像自動切換**：主域名連接失敗時，自動探測並切換至可用鏡像域名。
- **SupJav 多源處理**：支援 `TV` / `FST` / `VOE` / `ST` 多個伺服器檢測與自動故障轉移；自動剝離切片中偽裝的 PNG 圖片標頭資料。
- **預告片自動過濾**：自動跳過低於 600 秒的短預覽影片並切換至正片資料源。

### 介面與互動

- **極致微小與低耗資源**：安裝包與二進位檔僅約 **10 MB+** 級別，記憶體佔用相比傳統 Electron 降低超過 **80%**，毫秒級極速啟動。
- **響應式網格佈局**：根據視窗尺寸自動調整列數，避免大螢幕封面拉伸模糊和小螢幕文字受擠壓。
- **卡片懸停預覽**：懸停卡片時載入影片預覽片段。
- **批量任務控制與雙區管理**：提供下載佇列雙行緊湊佈局與 Hover 工具提示，支援全選、多選以及批量開始、暫停、取消和一鍵清空紀錄。
- **扁平化設計與多主題支援**：現代扁平化視覺規範，內建深色模式與淺色模式，支援一鍵即時無縫切換。
- **磁碟儲存分析**：獲取目標磁碟區的總容量、剩餘可用空間及下載檔案佔用比例。
- **多語言支援**：支援繁體中文、簡體中文、英文和日文介面，支援多語言影片標題。

---

## 技術棧

- **桌面框架**：Tauri v2
- **前端框架**：React 19, TypeScript, Vite
- **UI 與樣式**：Tailwind CSS v4, shadcn/ui, Lucide React
- **狀態管理**：Zustand
- **後端**：Rust (2021)
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

## 我該下載哪個版本

請根據您的作業系統與架構選擇對應的 Release 安裝包或免安裝可攜版檔案：

| 作業系統 | 架構 | 檔案格式 / 檔案名稱範例 | 說明與使用方式 |
| :--- | :--- | :--- | :--- |
| **Windows** | x64 (64位元) | `avdl_*_x64-setup.exe` | **安裝版**：雙擊執行安裝精靈即可完成安裝 |
| | | `AVDL_*_windows_x64.exe` | **可攜版（免安裝）**：直接雙擊執行即可使用 |
| **macOS** | Apple Silicon (arm64, M系列晶片) | `avdl_*.dmg` | **鏡像安裝包**：雙擊掛載 DMG 並拖入 Applications 資料夾 |
| | | `AVDL_*_mac_arm64.zip` | **可攜壓縮包**：解壓後將 `avdl.app` 移至應用程式資料夾使用<br>（首次執行若受權限攔截，請參考下方 macOS 說明） |
| **Linux** | x64 (64位元) | `avdl_*.AppImage` | **標準安裝包**：AppImage 賦予執行權限即可執行 |
| | | `AVDL_*_linux_x64.tar.gz` | **可攜壓縮包**：解壓提取 `avdl` 二進位檔案後直接執行 |

> 檔案名稱前綴大小寫與 Release 實際產物一致：`avdl_`（小寫）為 Tauri 自動生成的安裝包（NSIS / DMG / AppImage），`AVDL_`（大寫）為 CI 生成的可攜版。

---

## macOS 使用說明與常見問題

在運行打包後的 `.app` 或二進位檔案時請注意，軟體為開源未簽名應用，首次運行前如遇權限攔截，可在終端機執行命令解除 macOS 隔離限制：

```bash
# 解除應用包隔離
xattr -cr /Applications/avdl.app

# 解除單檔案二進位隔離
xattr -cr ./avdl
```

---

## 免責聲明

本專案僅供個人學習與技術研究使用
