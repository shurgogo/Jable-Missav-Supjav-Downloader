#!/usr/bin/env node
/**
 * 本地模拟 GitHub releases API，用于在发布前测试 AVDL 的版本更新检查功能。
 *
 * 用法：
 *   1. 启动本脚本：        node scripts/mock-release-server.mjs [端口，默认 8765]
 *   2. 另开终端启动应用：  AVDL_RELEASE_API_URL=http://127.0.0.1:8765 npm run tauri dev
 *   3. 应用启动后会请求本 server，发现 "v9.9.9" 比本地版本新 → 侧边栏出现 NEW 徽章。
 *
 * 修改下面 release 对象即可模拟不同场景：
 *   - tag_name 改成比本地版本低的版本 → 测试"已是最新"路径
 *   - prerelease: true → 测试 pre-release 回退逻辑（会改用 releases 列表）
 *   - body 填假 changelog → 测试对话框里的更新内容渲染
 *   - html_url 指向任意地址 → 测试"立即更新"打开的链接
 */
import http from "node:http";

const PORT = Number(process.argv[2] || 8765);

const release = {
  tag_name: "v9.9.9",
  name: "AVDL v9.9.9",
  body: `## 本次更新 (v9.9.9)

### 新功能
- 增加版本更新检查
- 支持 fMP4 视频下载

### 修复
- 修复下载速度显示为 0 的问题
- 修复非 TS 分片下载失败的问题

### 其他
- 稳定性优化`,
  html_url: `http://127.0.0.1:${PORT}/releases/tag/v9.9.9`,
  published_at: new Date().toISOString(),
  prerelease: false,
  draft: false,
};

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
  console.log(`[mock-release] ${req.method} ${url.pathname}${url.search}`);

  // 模拟 GitHub API
  if (url.pathname.endsWith("/releases/latest")) {
    res.setHeader("Content-Type", "application/json");
    return res.end(JSON.stringify(release));
  }
  if (url.pathname.endsWith("/releases")) {
    res.setHeader("Content-Type", "application/json");
    // prerelease: true 时，GitHub 的 /releases/latest 会返回 prerelease，
    // 应用会回退到这个列表找正式版 —— 把正式版放第一个即可测该逻辑。
    return res.end(JSON.stringify([release]));
  }

  // 其他路径（例如"立即更新"打开的 html_url）给一个简单页面
  res.setHeader("Content-Type", "text/html; charset=utf-8");
  res.end(
    `<!doctype html><html><body style="font-family:sans-serif;padding:40px">
      <h2>AVDL 本地更新测试页</h2>
      <p>「立即更新」按钮成功调用了 openUrl，打开了这个地址。</p>
      <p>请求路径：<code>${url.pathname}</code></p>
    </body></html>`
  );
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`✔ Mock GitHub release API: http://127.0.0.1:${PORT}`);
  console.log(`  当前模拟版本: ${release.tag_name} (prerelease=${release.prerelease})`);
  console.log(`  启动应用时设置: AVDL_RELEASE_API_URL=http://127.0.0.1:${PORT}`);
});
