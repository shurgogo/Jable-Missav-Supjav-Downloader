# AVDL - Video Downloader

Language: [简体中文](../README.md) | [繁體中文](README.zh-TW.md) | **English** | [日本語](README.ja.md)

A modern desktop video downloader and browser built with Tauri v2, React, and Rust, supporting JableTV, MissAV, and SupJav.

![Light Theme](../docs/images/preview01.png)

![Dark Theme](../docs/images/preview02.png)

---

## Features

### Downloading & File Processing
- **M3U8 Segment Download & Decryption**: Supports multi-threaded TS segment concurrent downloading, AES-128-CBC automatic decryption, and breakpoint resume.
- **Faststart Seeking Optimization**: Appends `-movflags +faststart` during MP4 merging to optimize keyframe index positioning, enabling instant playback and seeking.
- **Streamtape Progressive Streaming**: Decrypts Streamtape obfuscated scripts and streams data directly to disk.

### Anti-Crawler & Site Adaptation
- **Cloudflare Verification & Status Perception**: Built-in verification window for retrieving and managing `cf_clearance` cookies; automatically invalidates stale credentials on `403 Forbidden` and prompts for re-verification.
- **MissAV Mirror Auto-Switching**: Automatically detects connection failures and switches to available active mirror domains.
- **Referer Anti-Leech Handling**: Automatically attaches valid `Referer` headers to resolve preview and segment downloading blocks.
- **SupJav Multi-Server Probing**: Supports `TV` / `FST` / `VOE` / `ST` multi-server probing and automatic failover; strips obfuscated fake PNG headers from TS segments.
- **Short Preview Video Filter**: Calculates total M3U8 duration and automatically skips short preview clips (< 600 seconds) to target full-length video servers.

### Interface & User Experience
- **Responsive Fluid Grid**: Uses CSS Grid `auto-fill` (`minmax(240px, 1fr)`) to dynamically adjust layout columns based on window dimensions.
- **Card Hover Video Preview**: Loads video preview clips on hover and renders via in-memory Blob URLs.
- **Batch Download Queue Control**: Compact 2-row item layout with rich tooltips, select-all / multi-select checkboxes, and batch start, pause, and cancel operations.
- **Disk Storage Health Analytics**: Displays target storage disk partition capacity, free space, and download ratio.
- **Multi-Language Support**: Full localization for Traditional Chinese, Simplified Chinese, English, and Japanese.

---

## Tech Stack

- **Desktop Framework**: Tauri v2
- **Frontend**: React 19, TypeScript, Vite
- **UI & Styling**: Tailwind CSS, daisyUI, Lucide React
- **State Management**: Zustand
- **Backend**: Rust (Edition 2021)
- **Networking & Cryptography**: `reqwest` / `wreq`, `scraper`, `aes` / `cbc`
- **Video Merging**: FFmpeg (requires `ffmpeg` in system PATH)

---

## Build & Running

### Requirements
- Node.js (v18+)
- Rust (1.75+)
- FFmpeg

### Development & Packaging

1. **Install Dependencies**
   ```bash
   npm install
   ```

2. **Start Development Server**
   ```bash
   npm run tauri dev
   ```

3. **Build Binary / Application**
   ```bash
   npm run tauri build
   ```

---

## 🍎 macOS Usage & Troubleshooting

When running the compiled `.app` or standalone binary on macOS, please note that the application is an open-source unsigned package. If macOS Gatekeeper blocks execution on first run, execute the following commands in Terminal:

```bash
# Clear quarantine attribute for app bundle
xattr -cr /Applications/avdl.app

# Clear quarantine attribute for standalone binary
xattr -cr ./avdl
```

---

## Disclaimer

This project is intended strictly for personal learning and technical research purposes.
