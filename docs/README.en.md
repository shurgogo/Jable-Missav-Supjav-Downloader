# AVDL - Video Downloader

🌐 Language: [简体中文](../README.md) | [繁體中文](README.zh-TW.md) | **English** | [日本語](README.ja.md)

> ⚡ **Ultra Lightweight · Maximum Performance** — A high-performance desktop video downloading and browsing tool built with Tauri v2, React, and Rust. Features a featherweight binary (~10 MB) with ultra-low memory footprint. Supports JableTV, MissAV, and SupJav.

![Light Mode](../docs/images/preview01.png)

![Dark Mode](../docs/images/preview02.png)

---

## ✨ Features

### 📥 Download & File Processing
- ⚡ **M3U8 Segment Download & Decryption**: Supports multi-threaded TS segment concurrent downloading, AES-128-CBC automatic decryption, and breakpoint resume.
- 🌊 **Streamtape Streaming Download**: Decrypts Streamtape obfuscated scripts and uses streaming incremental writes to disk.

### 🛡️ Site Adaptation & Anti-Crawling
- 🔒 **Cloudflare Verification & Status Perception**: Built-in verification window to obtain and manage Cloudflare credentials, automatically clearing invalid credentials and prompting re-verification.
- 🔄 **MissAV Mirror Auto-Switching**: Automatically detects connection failures on the primary domain and switches to available mirror domains.
- ⚙️ **SupJav Multi-Source Processing**: Supports detection and automatic failover across multiple servers (`TV` / `FST` / `VOE` / `ST`); automatically strips disguised PNG image header data from segments.
- 🎬 **Trailer Auto-Filtering**: Automatically skips short preview videos under 600 seconds and switches to full-length video sources.

### 🎨 Interface & Interaction
- 🪶 **Ultra Lightweight & Low Footprint**: Powered by native Tauri v2 architecture, the executable package is only **~10 MB+**, consuming over **80% less memory** than Electron apps with millisecond startup speeds.
- 📐 **Responsive Grid Layout**: Automatically adjusts column count based on window size to prevent cover image stretching on large screens and text compression on small screens.
- 👁️ **Card Hover Preview**: Loads video preview clips when hovering over cards.
- 📋 **Batch Task Control & Dual-Section Management**: Provides a 2-row compact layout for the download queue with hover tooltips, supporting select-all, multi-select, and batch start, pause, cancel, and one-click history clearing.
- 🎨 **Flat Design & Multi-Theme Support**: Designed with shadcn/ui flat visual guidelines, offering built-in dark and light modes with seamless one-click switching.
- 📊 **Disk Storage Analytics**: Displays the total capacity, remaining available space, and download file usage ratio of the target drive.
- 🌐 **Multi-Language Support**: Supports Traditional Chinese, Simplified Chinese, English, and Japanese interfaces, as well as multi-language video titles.

---

## 🛠️ Tech Stack

- 🖥️ **Desktop Framework**: Tauri v2
- ⚛️ **Frontend Framework**: React 19, TypeScript, Vite
- 🎨 **UI & Styling**: Tailwind CSS v4, shadcn/ui, Lucide React
- 🐻 **State Management**: Zustand
- 🦀 **Backend**: Rust(2021)
- 🌐 **Networking & Decryption**: `reqwest` / `wreq`, `scraper`, `aes` / `cbc`
- 🎞️ **Video Merging**: FFmpeg (requires `ffmpeg` in system PATH)

---

## 🚀 Build & Running

### 📋 Requirements
- Node.js (v18+)
- Rust (1.75+)
- FFmpeg

### 📦 Development & Packaging

1. **Install Dependencies**
   ```bash
   npm install
   ```

2. **Start Development Server**
   ```bash
   npm run tauri dev
   ```

3. **Build Executable**
   ```bash
   npm run tauri build
   ```

---

## 🍎 macOS Usage Notes & FAQ

When running the packaged `.app` or binary file, please note that the software is an open-source unsigned application. If you encounter permission blocks prior to first launch, you can execute the following commands in the terminal to remove macOS quarantine restrictions:

```bash
# Remove app bundle quarantine
xattr -cr /Applications/avdl.app

# Remove single binary file quarantine
xattr -cr ./avdl
```

---

## 📜 Disclaimer

This project is intended strictly for personal learning and technical research purposes.
