import React, { useState, useEffect, useRef } from "react";
import { Settings as SettingsIcon, Info, ShieldCheck, CheckCircle2, FolderOpen, ExternalLink, FileText, Bug, Rocket, Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useDownloadStore } from "../store/useDownloadStore";
import { useToastStore } from "../store/useToastStore";
import { useUpdateStore } from "../store/useUpdateStore";
import { runUpdateCheck } from "../utils/update";
import { useTranslation } from "../i18n";
import { Card, CardHeader, CardTitle, CardContent } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Switch } from "../components/ui/switch";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "../components/ui/table";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "../components/ui/select";

export const Settings: React.FC = () => {
  const { t } = useTranslation();
  const showError = useToastStore((state) => state.showError);
  const showSuccess = useToastStore((state) => state.showSuccess);
  const settings = useDownloadStore((state) => state.settings);
  const updateSettings = useDownloadStore((state) => state.updateSettings);
  const tasks = useDownloadStore((state) => state.tasks);
  const completedTasks = useDownloadStore((state) => state.completedTasks);
  const activeSite = useDownloadStore((state) => state.activeSite);

  const [showToast, setShowToast] = useState<boolean>(false);
  const [lastLogPath, setLastLogPath] = useState<string | null>(null);
  const [appVersion, setAppVersion] = useState<string>("0.1.2");
  const toastTimeoutRef = useRef<any>(null);

  // Update availability state (shared with the sidebar badge)
  const updateInfo = useUpdateStore((s) => s.updateInfo);
  const checking = useUpdateStore((s) => s.checking);
  const ignoredVersion = useUpdateStore((s) => s.ignoredVersion);
  const setDialogOpen = useUpdateStore((s) => s.setDialogOpen);
  const clearIgnoredVersion = useUpdateStore((s) => s.clearIgnoredVersion);

  useEffect(() => {
    getVersion().then((v) => setAppVersion(v)).catch(() => {});
    return () => {
      if (toastTimeoutRef.current) {
        clearTimeout(toastTimeoutRef.current);
      }
    };
  }, []);

  const handleCheckUpdate = async () => {
    // Manual checks always surface the newest release, even for a version the
    // user previously skipped — so skipping is never a dead end.
    const { info, failed } = await runUpdateCheck({ respectIgnored: false });
    if (info) {
      setDialogOpen(true);
    } else if (failed) {
      showError(t("update_check_failed"));
    } else {
      showSuccess(t("update_manual_up_to_date", { version: appVersion }));
    }
  };

  const handleRestoreReminders = async () => {
    clearIgnoredVersion();
    const { info } = await runUpdateCheck();
    if (info) {
      setDialogOpen(true);
    } else {
      showSuccess(t("update_manual_up_to_date", { version: appVersion }));
    }
  };

  const triggerAutoSaveToast = () => {
    if (toastTimeoutRef.current) {
      clearTimeout(toastTimeoutRef.current);
    }
    setShowToast(true);
    toastTimeoutRef.current = setTimeout(() => {
      setShowToast(false);
    }, 1500);
  };

  const handleGenerateLog = async (overrideFolder?: string) => {
    try {
      const folder = overrideFolder || settings.downloadFolder;
      const activeTasksCount = Object.keys(tasks).length;
      const completedTasksCount = Object.keys(completedTasks).length;
      
      const cfDomains = Object.keys(settings.cfConfigs || {}).join(", ") || "None";
      
      const taskSummary = Object.values(tasks)
        .map((t) => `  - Title: ${t.title} | Status: ${t.status} | Progress: ${t.index}/${t.total}`)
        .join("\n") || "  None";

      const completedSummary = Object.values(completedTasks)
        .map((t) => `  - Title: ${t.title} | Status: ${t.status}`)
        .join("\n") || "  None";

      const logContent = [
        "==================================================",
        "          AVDL Diagnostic Debug Log               ",
        "==================================================",
        `Timestamp:           ${new Date().toISOString()} (${new Date().toLocaleString()})`,
        `App Version:         v${appVersion}`,
        `User Agent:          ${navigator.userAgent}`,
        `Active Site:         ${activeSite}`,
        `Download Folder:     ${folder}`,
        `Max Concurrent:      ${settings.maxConcurrent}`,
        `Resolution:          ${settings.resolution}`,
        `Theme:               ${settings.theme}`,
        `Language:            ${settings.language}`,
        `Logging Enabled:     ${Boolean(settings.enableLogging)}`,
        `CF Verified Domains: ${cfDomains}`,
        "--------------------------------------------------",
        `Active Tasks (${activeTasksCount}):`,
        taskSummary,
        "--------------------------------------------------",
        `Completed Tasks (${completedTasksCount}):`,
        completedSummary,
        "==================================================",
      ].join("\n");

      const path: string = await invoke("generate_debug_log", {
        saveDir: folder,
        logContent,
      });

      setLastLogPath(path);
      showSuccess(`${t("settings_logging_toast")}: ${path.split("/").pop() || path}`);
    } catch (err) {
      console.error("Failed to generate debug log:", err);
      showError(`生成排查日志失败: ${String(err)}`);
    }
  };

  const handleLoggingToggle = async (checked: boolean) => {
    updateSettings({ enableLogging: checked });
    triggerAutoSaveToast();
    if (checked) {
      await handleGenerateLog();
    } else {
      setLastLogPath(null);
    }
  };

  const handleAutoVerify = async (urlStr: string) => {
    try {
      await invoke("start_cf_verifier", {
        urlStr,
        userAgent: navigator.userAgent,
      });
    } catch (err) {
      console.error("Failed to start Cloudflare verifier:", err);
    }
  };

  const handleClearVerify = (domain: string) => {
    const newConfigs = { ...(settings.cfConfigs || {}) };
    delete newConfigs[domain];
    updateSettings({
      cfConfigs: newConfigs,
    });
    triggerAutoSaveToast();
  };

  const handleBrowse = async () => {
    try {
      const selected: string | null = await invoke("select_directory");
      if (selected) {
        updateSettings({ downloadFolder: selected });
        triggerAutoSaveToast();
      }
    } catch (err) {
      console.error("Failed to select directory:", err);
      showError(err);
    }
  };

  const handleOpenFolder = async () => {
    try {
      await invoke("open_download_folder", {
        saveDir: settings.downloadFolder,
      });
    } catch (err) {
      console.error("Failed to open download folder:", err);
      showError(err);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-screen overflow-hidden bg-background text-foreground relative">

      {/* Toast Alert */}
      {showToast && (
        <div className="fixed top-4 right-4 z-50 animate-in fade-in slide-in-from-top-4">
          <div className="bg-emerald-600 text-white shadow-lg rounded-xl flex items-center gap-2 px-4 py-3 font-bold text-sm">
            <CheckCircle2 className="w-5 h-5" />
            <span>{t("settings_autosave_toast")}</span>
          </div>
        </div>
      )}

      {/* Header */}
      <header className="p-5 border-b border-border bg-card/40 backdrop-blur-md flex items-center justify-between shrink-0 select-none">
        <div className="flex items-center gap-2.5">
          <SettingsIcon className="w-5 h-5 text-primary" />
          <h2 className="text-lg font-extrabold">{t("settings_title")}</h2>
        </div>
        <Badge variant="success" className="font-extrabold px-3 py-1 flex items-center gap-1.5 animate-pulse">
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
          {t("settings_autosave_badge")}
        </Badge>
      </header>

      {/* Settings Form Grid */}
      <main className="flex-1 overflow-y-auto p-6">
        <div className="max-w-3xl mx-auto space-y-6">

          {/* Card: Download Preferences */}
          <Card>
            <CardHeader className="pb-4">
              <CardTitle className="text-base font-extrabold text-primary border-b border-border pb-2">
                {t("nav_settings")}
              </CardTitle>
            </CardHeader>

            <CardContent className="space-y-6">
              {/* Setting row: folder */}
              <div className="space-y-2">
                <label className="text-sm font-extrabold text-foreground block">
                  {t("settings_folder")}
                </label>
                <div className="flex flex-col sm:flex-row items-center gap-2">
                  <Input
                    type="text"
                    value={settings.downloadFolder}
                    onChange={(e) => {
                      updateSettings({ downloadFolder: e.target.value });
                      triggerAutoSaveToast();
                    }}
                    className="flex-1 font-bold"
                    placeholder="例如: download 或 /Users/username/Downloads/my_videos"
                  />
                  <div className="flex items-center gap-2 shrink-0 w-full sm:w-auto">
                    <Button
                      onClick={handleBrowse}
                      className="font-bold flex-1 sm:flex-none"
                    >
                      <FolderOpen className="w-4 h-4 mr-1" />
                      {t("settings_folder_browse")}
                    </Button>
                    <Button
                      variant="secondary"
                      onClick={handleOpenFolder}
                      className="font-bold flex-1 sm:flex-none"
                    >
                      <ExternalLink className="w-4 h-4 mr-1 text-muted-foreground" />
                      {t("settings_folder_open")}
                    </Button>
                  </div>
                </div>
                <p className="text-xs text-muted-foreground font-medium">
                  {t("settings_folder_desc")}
                </p>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                {/* Setting: concurrent */}
                <div className="space-y-2">
                  <label className="text-sm font-extrabold text-foreground block">
                    {t("settings_concurrent")}
                  </label>
                  <Select
                    value={String(settings.maxConcurrent)}
                    onValueChange={(val) => {
                      updateSettings({ maxConcurrent: Number(val) });
                      triggerAutoSaveToast();
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {[1, 2, 3, 4, 5, 6, 8, 10, 12, 16].map((num) => (
                        <SelectItem key={num} value={String(num)}>
                          {num}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground font-medium">
                    {t("settings_concurrent_desc")}
                  </p>
                </div>

                {/* Setting: resolution */}
                <div className="space-y-2">
                  <label className="text-sm font-extrabold text-foreground block">
                    {t("settings_resolution")}
                  </label>
                  <Select
                    value={settings.resolution}
                    onValueChange={(val) => {
                      updateSettings({ resolution: val });
                      triggerAutoSaveToast();
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="highest">Highest</SelectItem>
                      <SelectItem value="1080">1080P</SelectItem>
                      <SelectItem value="720">720P</SelectItem>
                      <SelectItem value="lowest">Lowest</SelectItem>
                    </SelectContent>
                  </Select>
                  <p className="text-xs text-muted-foreground font-medium">
                    {t("settings_resolution_desc")}
                  </p>
                </div>
              </div>

              {/* Setting: Theme selection */}
              <div className="space-y-2 border-t border-border pt-4">
                <label className="text-sm font-extrabold text-foreground block">
                  {t("settings_theme")}
                </label>
                <Select
                  value={settings.theme || "dark"}
                  onValueChange={(val) => {
                    updateSettings({ theme: val });
                    triggerAutoSaveToast();
                  }}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="dark">🌙 Dark (暗黑)</SelectItem>
                    <SelectItem value="light">☀️ Light (明亮)</SelectItem>
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground font-medium">
                  {t("settings_theme_desc")}
                </p>
              </div>

              {/* Setting: Language selection */}
              <div className="space-y-2 border-t border-border pt-4">
                <label className="text-sm font-extrabold text-foreground block">
                  {t("settings_lang")}
                </label>
                <Select
                  value={settings.language || "zh-TW"}
                  onValueChange={(val) => {
                    const newLang = val;
                    updateSettings({ language: newLang });
                    triggerAutoSaveToast();
                  }}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="zh-TW">繁體中文 (Traditional Chinese)</SelectItem>
                    <SelectItem value="zh-CN">简体中文 (Simplified Chinese)</SelectItem>
                    <SelectItem value="en">English (English)</SelectItem>
                    <SelectItem value="ja">日本語 (Japanese)</SelectItem>
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground font-medium">
                  {t("settings_lang_desc")}
                </p>
              </div>

              {/* Setting: Generate Log File (Troubleshooting) */}
              <div className="border-t border-border pt-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5 pr-4">
                    <label className="text-sm font-extrabold text-foreground flex items-center gap-2 cursor-pointer">
                      <Bug className="w-4 h-4 text-primary" />
                      <span>{t("settings_logging")}</span>
                    </label>
                    <p className="text-xs text-muted-foreground font-medium">
                      {t("settings_logging_desc")}
                    </p>
                  </div>
                  <Switch
                    checked={Boolean(settings.enableLogging)}
                    onCheckedChange={handleLoggingToggle}
                  />
                </div>

                {Boolean(settings.enableLogging) && (
                  <div className="flex items-center justify-between bg-muted/40 p-3 rounded-xl border border-border mt-2 animate-fade-in">
                    <div className="flex items-center gap-2 text-xs font-mono truncate text-muted-foreground">
                      <FileText className="w-4 h-4 text-primary shrink-0" />
                      <span className="truncate">
                        {lastLogPath ? lastLogPath.split("/").pop() : "即将写入日志至下载目录..."}
                      </span>
                    </div>
                    <Button
                      size="xs"
                      variant="outline"
                      onClick={() => handleGenerateLog()}
                      className="font-bold shrink-0 ml-2"
                    >
                      {t("settings_logging_btn")}
                    </Button>
                  </div>
                )}
              </div>

            </CardContent>
          </Card>

          {/* Card: Cloudflare Bypass Check */}
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base font-extrabold text-primary border-b border-border pb-2 flex items-center gap-2">
                <ShieldCheck className="w-5 h-5" />
                {t("settings_cf")}
              </CardTitle>
            </CardHeader>

            <CardContent className="space-y-4">
              <p className="text-xs text-muted-foreground font-semibold">
                {t("settings_cf_desc")}
              </p>

              <div className="border border-border rounded-xl overflow-hidden bg-muted/20">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Site</TableHead>
                      <TableHead>Domain</TableHead>
                      <TableHead>Token Status</TableHead>
                      <TableHead>Action</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {[
                      { name: "JableTV", domain: "jable.tv", url: "https://jable.tv/" },
                      { name: "MissAV", domain: "missav.ws", url: "https://missav.ws/" },
                      { name: "SupJav", domain: "supjav.com", url: "https://supjav.com/" },
                    ].map((site) => {
                      const cfg = settings.cfConfigs?.[site.domain];
                      const hasCookie = !!cfg?.cfClearance;
                      return (
                        <TableRow key={site.domain}>
                          <TableCell className="font-extrabold text-sm">{site.name}</TableCell>
                          <TableCell className="text-xs font-mono">{site.domain}</TableCell>
                          <TableCell>
                            {hasCookie ? (
                              <Badge variant="success" className="font-semibold text-[11px] truncate max-w-[150px]" title={cfg.cfClearance}>
                                Authorized ({cfg.cfClearance.substring(0, 10)}...)
                              </Badge>
                            ) : (
                              <Badge variant="secondary" className="font-semibold text-[11px]">Unauthorized</Badge>
                            )}
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center gap-2">
                              <Button
                                size="xs"
                                onClick={() => handleAutoVerify(site.url)}
                                className="font-extrabold"
                              >
                                {t("settings_cf_verify")}
                              </Button>
                              {hasCookie && (
                                <Button
                                  size="xs"
                                  variant="outline"
                                  onClick={() => handleClearVerify(site.domain)}
                                  className="font-bold"
                                >
                                  {t("settings_cf_clear")}
                                </Button>
                              )}
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>

              <div className="flex items-start gap-3 bg-muted/30 p-4 rounded-xl border border-border">
                <Info className="w-5 h-5 text-primary shrink-0 mt-0.5" />
                <div className="text-xs text-muted-foreground font-medium space-y-1">
                  <p>Client engine emulates Chrome 120 client fingerprints to bypass Cloudflare protection.</p>
                  <p>Click "Verify" to open target check window, complete Turnstile challenge inside the window. The window closes automatically once verification is successful.</p>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Card: About & Updates */}
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base font-extrabold text-primary border-b border-border pb-2 flex items-center gap-2">
                <Rocket className="w-5 h-5" />
                {t("update_about_title")}
              </CardTitle>
            </CardHeader>

            <CardContent>
              <div className="flex flex-wrap items-center justify-between gap-4">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="w-11 h-11 rounded-xl bg-primary/10 text-primary flex items-center justify-center shrink-0">
                    <Rocket className="w-5 h-5" />
                  </div>
                  <div className="min-w-0">
                    <p className="font-extrabold text-sm">AVDL</p>
                    <p className="text-xs text-muted-foreground font-semibold">
                      v{appVersion}
                    </p>
                    {updateInfo ? (
                      <button
                        onClick={() => setDialogOpen(true)}
                        className="mt-0.5 inline-flex items-center gap-1 text-[11px] font-extrabold text-primary hover:text-primary/80 cursor-pointer"
                      >
                        <CheckCircle2 className="w-3 h-3" />
                        {t("update_title")} {updateInfo.latestVersion}
                      </button>
                    ) : ignoredVersion ? (
                      <span className="mt-0.5 inline-flex items-center gap-1.5 text-[11px] text-muted-foreground/70 font-medium">
                        {t("update_ignored_hint", { version: ignoredVersion })}
                        <button
                          onClick={handleRestoreReminders}
                          className="text-primary hover:text-primary/80 font-bold cursor-pointer underline underline-offset-2"
                        >
                          {t("update_restore")}
                        </button>
                      </span>
                    ) : (
                      <p className="mt-0.5 text-[11px] text-muted-foreground/70 font-medium">
                        {t("update_up_to_date")}
                      </p>
                    )}
                  </div>
                </div>

                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleCheckUpdate}
                  disabled={checking}
                  className="font-bold gap-1.5"
                >
                  {checking ? (
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  ) : (
                    <Rocket className="w-3.5 h-3.5" />
                  )}
                  {checking ? t("update_checking") : t("update_check_btn")}
                </Button>
              </div>
            </CardContent>
          </Card>

        </div>
      </main>
    </div>
  );
};
