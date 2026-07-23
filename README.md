# AVDL - 视频下载工具

🌐 Language: **简体中文** | [繁體中文](docs/README.zh-TW.md) | [English](docs/README.en.md) | [日本語](docs/README.ja.md)

基于 Tauri v2、React 和 Rust 构建的高性能桌面视频下载与浏览工具，支持 JableTV、MissAV 和 SupJav。

![浅色模式](./docs/images/preview01.png)

![深色模式](./docs/images/preview02.png)

---

## ✨ 功能特性

### 📥 下载与文件处理
- ⚡ **M3U8 切片下载与解密**：支持 TS 分片多线程并发下载、AES-128-CBC 自动解密与断点续传。
- ⏩ **Faststart 播放优化**：合并 MP4 时追加 `-movflags +faststart` 参数，优化 keyframe 索引表位置，使生成的 MP4 支持拖动进度条即时播放。
- 🌊 **Streamtape 流式下载**：解密 Streamtape 混淆脚本，采用流式增量写入磁盘。

### 🛡️ 站点适配与防爬处理
- 🔒 **Cloudflare 验证与状态感知**：内置验证窗口以获取和管理 `cf_clearance` 凭证；当请求返回 `403 Forbidden` 时自动清除失效凭证并提示重新验证。
- 🔄 **MissAV 镜像自动切换**：主域名连接失败时，自动探测并切换至可用镜像域名。
- 🔗 **Referer 防盗链处理**：请求资源时自动附加正确的 `Referer` 标头，解决预览与切片下载被拦截的问题。
- ⚙️ **SupJav 多源处理**：支持 `TV` / `FST` / `VOE` / `ST` 多个服务器检测与自动故障转移；自动剥离切片中伪装的 PNG 图片头数据。
- 🎬 **预告片自动过滤**：解析 M3U8 总时长，自动跳过低于 600 秒的短预览视频并切换至正片数据源。

### 🎨 界面与交互
- 📐 **响应式网格布局**：采用 CSS Grid `auto-fill`（最小宽度 240px），根据窗口尺寸自动调整列数，避免大屏封面拉伸模糊和小屏文本受挤压。
- 👁️ **卡片悬停预览**：悬停卡片时加载视频预览片段并生成内存 Blob URL 播放。
- 📋 **批量任务控制与双区管理**：下载队列分为「进行中」与「已完成」两大区块，提供双行紧凑布局与 Hover 工具提示，支持全选、多选以及批量开始、暂停、取消和一键清空记录。
- 🎨 **多主题切换支持**：内置 daisyUI 多套主题外观（包含深色模式与浅色模式），支持一键实时无缝切换。
- 📊 **磁盘存储分析**：获取目标盘符的总容量、剩余可用空间及下载文件占用比例。
- 🌐 **多语言支持**：支持繁体中文、简体中文、英文和日语界面，支持多语言视频标题。

---

## 🛠️ 技术栈

- 🖥️ **桌面框架**：Tauri v2
- ⚛️ **前端框架**：React 19, TypeScript, Vite
- 🎨 **UI 与样式**：Tailwind CSS, daisyUI, Lucide React
- 🐻 **状态管理**：Zustand
- 🦀 **后端**：Rust
- 🌐 **网络与解密**：`reqwest` / `wreq`, `scraper`, `aes` / `cbc`
- 🎞️ **视频合并**：FFmpeg（需要系统 PATH 中包含 `ffmpeg`）

---

## 🚀 运行与构建

### 📋 依赖要求
- Node.js (v18+)
- Rust (1.75+)
- FFmpeg

### 📦 开发与打包

1. **安装依赖**
   ```bash
   npm install
   ```

2. **启动开发服务**
   ```bash
   npm run tauri dev
   ```

3. **构建可执行文件**
   ```bash
   npm run tauri build
   ```

---

## 🍎 macOS 使用说明与常见问题

在运行打包后的 `.app` 或二进制文件时请注意，软件为开源未签名应用，首次运行前如遇权限拦截，可在终端执行命令解除 macOS 隔离限制：

```bash
# 解除应用包隔离
xattr -cr /Applications/avdl.app

# 解除单文件二进制隔离
xattr -cr ./avdl
```

---

## 📜 免责声明

本项目仅供个人学习与技术研究使用。
