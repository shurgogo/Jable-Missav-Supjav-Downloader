import React, { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  DownloadCloud,
  Trash2,
  FolderOpen,
  Loader2,
  Play,
  AlertCircle,
  Pause,
  X,
  CheckCircle2,
} from "lucide-react";
import { useDownloadStore, Site } from "../store/useDownloadStore";
import { useToastStore } from "../store/useToastStore";
import { useTranslation } from "../i18n";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Progress } from "../components/ui/progress";
import { cn } from "../lib/utils";

interface ProgressPayload {
  url: string;
  title: string;
  index: number;
  total: number;
  speed_kbps: number;
  status: string;
}

export const Queue: React.FC = () => {
  const { t } = useTranslation();
  const showError = useToastStore((state) => state.showError);
  const tasks = useDownloadStore((state) => state.tasks);
  const completedTasks = useDownloadStore((state) => state.completedTasks);
  const updateTask = useDownloadStore((state) => state.updateTask);
  const removeTask = useDownloadStore((state) => state.removeTask);
  const clearCompletedTasks = useDownloadStore((state) => state.clearCompletedTasks);
  const settings = useDownloadStore((state) => state.settings);

  // Selected tasks state in Queue (for active tasks)
  const [selectedQueueUrls, setSelectedQueueUrls] = useState<string[]>([]);

  // Track active downloads via event listener
  useEffect(() => {
    const unlistenPromise = listen<ProgressPayload>("download-progress", (event) => {
      const { url, title, index, total, speed_kbps, status } = event.payload;
      if (status.startsWith("failed")) {
        showError(status.replace("failed: ", ""));
      }
      updateTask(url, {
        title: title || undefined,
        index,
        total,
        speedKbps: speed_kbps,
        status,
      });
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [updateTask, showError]);

  // Scan for interrupted tasks on disk upon component load
  useEffect(() => {
    const scanUnfinished = async () => {
      try {
        const list: Array<{
          site?: Site;
          url: string;
          title: string;
          save_dir: string;
          max_concurrent: number;
          resolution: string;
          completed_segments: number;
          total_segments: number;
        }> = await invoke("scan_unfinished_tasks", {
          saveDir: settings.downloadFolder,
        });

        for (const item of list) {
          if (!tasks[item.url] && !completedTasks[item.url]) {
            updateTask(item.url, {
              site: item.site,
              title: item.title,
              index: item.completed_segments,
              total: item.total_segments,
              speedKbps: 0,
              status: "paused",
            });
          }
        }
      } catch (err) {
        console.error("Failed to scan unfinished tasks:", err);
      }
    };
    scanUnfinished();
  }, [settings.downloadFolder, updateTask, tasks, completedTasks]);

  const handlePause = async (url: string) => {
    updateTask(url, { status: "paused", speedKbps: 0 });
    try {
      await invoke("pause_download", { url });
    } catch (err) {
      console.error("Failed to pause:", err);
    }
  };

  const activeSite = useDownloadStore((state) => state.activeSite);

  const handleResume = async (url: string) => {
    updateTask(url, { status: "downloading" });
    const task = tasks[url];
    const taskSite = task?.site || activeSite;
    const saveDir = settings.downloadFolder;
    const maxConcurrent = settings.maxConcurrent;
    const resolution = settings.resolution;

    try {
      await invoke("resume_download", {
        site: taskSite,
        url,
        saveDir,
        maxConcurrent,
        resolution,
      });
    } catch (err) {
      console.error("Failed to resume:", err);
      showError(err);
      updateTask(url, { status: `failed: ${err}` });
    }
  };

  const handleCancel = async (url: string) => {
    updateTask(url, { status: "paused", speedKbps: 0 });
    try {
      await invoke("cancel_download", {
        url,
        saveDir: settings.downloadFolder,
        title: tasks[url]?.title || "",
      });
      removeTask(url);
    } catch (err) {
      console.error("Failed to cancel download:", err);
    }
  };

  const handleOpenFolder = async () => {
    try {
      await invoke("open_download_folder", {
        saveDir: settings.downloadFolder,
      });
    } catch (err) {
      console.error("Failed to open download folder:", err);
    }
  };

  const activeTaskEntries = Object.entries(tasks);
  const completedTaskEntries = Object.entries(completedTasks);
  const hasAnyTasks = activeTaskEntries.length > 0 || completedTaskEntries.length > 0;

  const allActiveUrls = activeTaskEntries.map(([url]) => url);
  const allSelected =
    allActiveUrls.length > 0 && allActiveUrls.every((url) => selectedQueueUrls.includes(url));

  const toggleSelectAll = () => {
    if (allSelected) {
      setSelectedQueueUrls([]);
    } else {
      setSelectedQueueUrls(allActiveUrls);
    }
  };

  const toggleSelectTask = (url: string) => {
    setSelectedQueueUrls((prev) =>
      prev.includes(url) ? prev.filter((u) => u !== url) : [...prev, url]
    );
  };

  // Batch Handlers for Active Tasks
  const handleResumeSelected = async () => {
    const targets = selectedQueueUrls.length > 0 ? selectedQueueUrls : allActiveUrls;
    for (const url of targets) {
      const t = tasks[url];
      if (t && (t.status === "paused" || t.status === "pending" || t.status.startsWith("failed"))) {
        handleResume(url);
      }
    }
  };

  const handlePauseSelected = async () => {
    const targets = selectedQueueUrls.length > 0 ? selectedQueueUrls : allActiveUrls;
    for (const url of targets) {
      const t = tasks[url];
      if (t && t.status === "downloading") {
        handlePause(url);
      }
    }
  };

  const handleCancelSelected = async () => {
    const targets = [...(selectedQueueUrls.length > 0 ? selectedQueueUrls : allActiveUrls)];
    for (const url of targets) {
      handleCancel(url);
    }
    setSelectedQueueUrls([]);
  };

  const handleResumeAll = async () => {
    for (const [url, t] of activeTaskEntries) {
      if (t.status === "paused" || t.status === "pending" || t.status.startsWith("failed")) {
        handleResume(url);
      }
    }
  };

  const handlePauseAll = async () => {
    for (const [url, t] of activeTaskEntries) {
      if (t.status === "downloading") {
        handlePause(url);
      }
    }
  };

  const handleCancelAll = async () => {
    for (const [url] of activeTaskEntries) {
      handleCancel(url);
    }
    setSelectedQueueUrls([]);
  };

  const formatSpeed = (kbps: number): string => {
    if (!kbps || kbps <= 0) return "0.0 KB/s";
    if (kbps >= 1024) {
      return `${(kbps / 1024).toFixed(2)} MB/s`;
    }
    if (kbps < 1.0) {
      const Bps = Math.round(kbps * 1024);
      return `${Bps} B/s`;
    }
    return `${kbps.toFixed(1)} KB/s`;
  };

  const renderStatusBadge = (status: string, index: number) => {
    if (status === "pending") {
      return <Badge variant="secondary" className="font-bold text-xs select-none">{t("queue_status_pending")}</Badge>;
    }
    if (status === "downloading") {
      return <Badge variant="default" className="font-bold text-xs select-none bg-amber-500 hover:bg-amber-600 text-white">{t("queue_status_downloading")}</Badge>;
    }
    if (status === "paused") {
      if (index === 0) {
        return <Badge variant="outline" className="font-bold text-xs select-none">{t("queue_status_pending")}</Badge>;
      }
      return <Badge variant="outline" className="font-bold text-xs select-none border-amber-500 text-amber-500">{t("queue_status_paused")}</Badge>;
    }
    if (status === "merging") {
      return (
        <Badge variant="default" className="font-bold text-xs select-none bg-blue-600 hover:bg-blue-700 text-white flex items-center gap-1">
          <Loader2 className="w-3 h-3 animate-spin" /> {t("queue_status_merging")}
        </Badge>
      );
    }
    if (status === "completed") {
      return <Badge variant="success" className="font-bold text-xs select-none">{t("queue_status_completed")}</Badge>;
    }
    if (status.startsWith("failed")) {
      return <Badge variant="destructive" className="font-bold text-xs select-none" title={status}>{t("queue_status_failed")}</Badge>;
    }
    return <Badge variant="secondary" className="font-bold text-xs select-none">{status}</Badge>;
  };

  return (
    <div className="flex-1 flex flex-col h-screen overflow-hidden bg-background text-foreground">
      {/* Header */}
      <header className="p-5 border-b border-border bg-card/40 backdrop-blur-md flex items-center justify-between shrink-0 select-none">
        <div className="flex items-center gap-2.5">
          <DownloadCloud className="w-5 h-5 text-primary" />
          <h2 className="text-lg font-extrabold">{t("queue_title")}</h2>
        </div>
      </header>

      {/* Main Area */}
      <main className="flex-1 overflow-y-auto p-6 bg-background/50">
        {!hasAnyTasks ? (
          <div className="flex flex-col items-center justify-center h-full text-center select-none max-w-sm mx-auto">
            <DownloadCloud className="w-16 h-16 text-muted-foreground/30 mb-4 animate-bounce" />
            <h3 className="font-extrabold text-lg text-muted-foreground">{t("queue_empty")}</h3>
            <p className="text-sm text-muted-foreground/70 mt-1">
              {t("queue_empty_desc")}
            </p>
          </div>
        ) : (
          <div className="max-w-4xl mx-auto space-y-6">
            
            {/* SECTION 1: ACTIVE TASKS */}
            {activeTaskEntries.length > 0 && (
              <div className="space-y-4">
                <div className="flex items-center justify-between select-none">
                  <div className="flex items-center gap-2">
                    <span className="w-2.5 h-2.5 rounded-full bg-primary animate-pulse" />
                    <h3 className="font-extrabold text-sm text-foreground">
                      {t("queue_section_active")} ({activeTaskEntries.length})
                    </h3>
                  </div>
                </div>

                {/* Batch Action Toolbar */}
                <div className="flex flex-wrap items-center justify-between gap-3 p-3 bg-card/60 rounded-xl border border-border shrink-0 select-none shadow-sm">
                  <div className="flex items-center gap-3">
                    <label className="flex items-center gap-2 cursor-pointer font-bold text-xs select-none">
                      <input
                        type="checkbox"
                        checked={allSelected}
                        onChange={toggleSelectAll}
                        className="rounded border-input text-primary focus:ring-ring accent-primary cursor-pointer w-4 h-4"
                      />
                      <span>{t("browse_btn_select_all")}</span>
                    </label>
                    {selectedQueueUrls.length > 0 && (
                      <Badge variant="destructive" className="font-bold text-[11px]">
                        {t("queue_selected_count", {
                          selected: selectedQueueUrls.length,
                          total: activeTaskEntries.length,
                        })}
                      </Badge>
                    )}
                  </div>

                  <div className="flex items-center gap-2 flex-wrap">
                    {/* Selected Group */}
                    {selectedQueueUrls.length > 0 && (
                      <div className="flex items-center rounded-lg border border-border bg-background p-0.5 shadow-sm">
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={handleResumeSelected}
                          className="font-bold text-emerald-500 hover:text-emerald-600 hover:bg-emerald-500/10 gap-1"
                          title={t("queue_btn_start_selected")}
                        >
                          <Play className="w-3 h-3 fill-current" />
                          <span>{t("queue_btn_start_selected")}</span>
                        </Button>
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={handlePauseSelected}
                          className="font-bold text-amber-500 hover:text-amber-600 hover:bg-amber-500/10 gap-1"
                          title={t("queue_btn_pause_selected")}
                        >
                          <Pause className="w-3 h-3" />
                          <span>{t("queue_btn_pause_selected")}</span>
                        </Button>
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={handleCancelSelected}
                          className="font-bold text-destructive hover:bg-destructive/10 gap-1"
                          title={t("queue_btn_cancel_selected")}
                        >
                          <Trash2 className="w-3 h-3" />
                          <span>{t("queue_btn_cancel_selected")}</span>
                        </Button>
                      </div>
                    )}

                    {/* All Group */}
                    <div className="flex items-center rounded-lg border border-border bg-background p-0.5 shadow-sm">
                      <Button
                        size="xs"
                        variant="ghost"
                        onClick={handleResumeAll}
                        className="font-bold gap-1"
                        title={t("queue_btn_start_all")}
                      >
                        <Play className="w-3 h-3 fill-current text-emerald-500" />
                        <span>{t("queue_btn_start_all")}</span>
                      </Button>
                      <Button
                        size="xs"
                        variant="ghost"
                        onClick={handlePauseAll}
                        className="font-bold gap-1"
                        title={t("queue_btn_pause_all")}
                      >
                        <Pause className="w-3 h-3 text-amber-500" />
                        <span>{t("queue_btn_pause_all")}</span>
                      </Button>
                      <Button
                        size="xs"
                        variant="ghost"
                        onClick={handleCancelAll}
                        className="font-bold text-muted-foreground hover:text-destructive gap-1"
                        title={t("queue_btn_cancel_all")}
                      >
                        <Trash2 className="w-3 h-3" />
                        <span>{t("queue_btn_cancel_all")}</span>
                      </Button>
                    </div>
                  </div>
                </div>

                {/* Active Items List */}
                <div className="space-y-2.5">
                  {activeTaskEntries.map(([url, task]) => {
                    const percentage =
                      task.total > 0 ? Math.round((task.index / task.total) * 100) : 0;
                    const isFailed = task.status.startsWith("failed");
                    const isSelected = selectedQueueUrls.includes(url);

                    const tooltipText =
                      `标题: ${task.title}\n` +
                      `链接: ${url}\n` +
                      `进度: ${task.index}/${task.total} 切片 (${percentage}%)\n` +
                      `速度: ${formatSpeed(task.speedKbps)}\n` +
                      `保存位置: ${settings.downloadFolder}`;

                    return (
                      <div
                        key={url}
                        className={cn(
                          "p-3.5 bg-card rounded-xl flex flex-col gap-2.5 border transition-all duration-200 shadow-sm",
                          isSelected
                            ? "border-primary ring-1 ring-primary/30 bg-primary/5"
                            : isFailed
                            ? "border-destructive/30 bg-destructive/5"
                            : "border-border hover:border-muted-foreground/30"
                        )}
                      >
                        {/* Row 1: Checkbox + Title + Buttons */}
                        <div className="flex items-center justify-between gap-3 select-none">
                          <div className="flex items-center gap-2.5 min-w-0 flex-1">
                            <input
                              type="checkbox"
                              checked={isSelected}
                              onChange={() => toggleSelectTask(url)}
                              className="rounded border-input text-primary focus:ring-ring accent-primary cursor-pointer w-4 h-4 shrink-0"
                            />

                            <div className="min-w-0 flex-1" title={tooltipText}>
                              <h4 className="font-extrabold text-xs sm:text-sm truncate text-foreground cursor-help hover:text-primary transition-colors">
                                {task.title}
                              </h4>
                            </div>
                          </div>

                          {/* Status badge & Single task controls */}
                          <div className="flex items-center gap-1.5 shrink-0 select-none">
                            {renderStatusBadge(task.status, task.index)}

                            {task.status === "downloading" && (
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                onClick={() => handlePause(url)}
                                className="text-muted-foreground hover:text-amber-500"
                                title={t("queue_btn_pause")}
                              >
                                <Pause className="w-3.5 h-3.5" />
                              </Button>
                            )}

                            {task.status === "paused" && (
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                onClick={() => handleResume(url)}
                                className="text-muted-foreground hover:text-emerald-500"
                                title={t("queue_btn_resume")}
                              >
                                <Play className="w-3.5 h-3.5 fill-current" />
                              </Button>
                            )}

                            <Button
                              variant="ghost"
                              size="icon-sm"
                              onClick={() => handleCancel(url)}
                              className="text-muted-foreground hover:text-destructive"
                              title={t("queue_btn_cancel")}
                            >
                              <X className="w-3.5 h-3.5" />
                            </Button>
                          </div>
                        </div>

                        {/* Row 2: Progress Bar + Speed */}
                        {!isFailed && (
                          <div className="flex items-center gap-3 w-full pl-6">
                            <Progress value={percentage} className="flex-1 h-2" />
                            <div className="flex items-center gap-2 font-mono text-[11px] font-bold shrink-0">
                              <span className="text-muted-foreground">{percentage}%</span>
                              {task.status === "downloading" && (
                                <span className="text-primary font-bold">{formatSpeed(task.speedKbps)}</span>
                              )}
                            </div>
                          </div>
                        )}

                        {/* Error Banner if Failed */}
                        {isFailed && (
                          <div className="text-xs text-destructive font-bold bg-destructive/10 p-2 rounded-lg border border-destructive/20 flex items-center gap-2 mt-1">
                            <AlertCircle className="w-3.5 h-3.5 shrink-0" />
                            <span>
                              {task.status.replace("failed: ", `${t("queue_status_failed")}: `)}
                            </span>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* SECTION 2: COMPLETED TASKS */}
            {completedTaskEntries.length > 0 && (
              <div className="space-y-4 pt-2">
                <div className="flex items-center justify-between select-none">
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="w-4 h-4 text-emerald-500" />
                    <h3 className="font-extrabold text-sm text-foreground">
                      {t("queue_section_completed")} ({completedTaskEntries.length})
                    </h3>
                  </div>

                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={clearCompletedTasks}
                    className="text-muted-foreground hover:text-destructive gap-1 font-bold"
                    title={t("queue_btn_clear_completed")}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                    <span>{t("queue_btn_clear_completed")}</span>
                  </Button>
                </div>

                {/* Completed Items List */}
                <div className="space-y-2.5">
                  {completedTaskEntries.map(([url, task]) => {
                    const tooltipText =
                      `标题: ${task.title}\n` +
                      `链接: ${url}\n` +
                      `状态: 已完成 (100%)\n` +
                      `保存位置: ${settings.downloadFolder}`;

                    return (
                      <div
                        key={url}
                        className="p-3.5 bg-card/70 rounded-xl flex items-center justify-between gap-3 border border-border hover:border-muted-foreground/30 transition-all shadow-sm"
                      >
                        <div className="flex items-center gap-2.5 min-w-0 flex-1 select-none">
                          <CheckCircle2 className="w-4 h-4 text-emerald-500 shrink-0" />

                          <div className="min-w-0 flex-1" title={tooltipText}>
                            <h4 className="font-extrabold text-xs sm:text-sm truncate text-foreground hover:text-primary cursor-help transition-colors">
                              {task.title}
                            </h4>
                          </div>
                        </div>

                        <div className="flex items-center gap-2 shrink-0 select-none">
                          <Badge variant="success" className="font-bold text-xs">
                            {t("queue_status_completed")}
                          </Badge>

                          <Button
                            variant="ghost"
                            size="icon-sm"
                            onClick={handleOpenFolder}
                            className="text-muted-foreground hover:text-primary"
                            title={t("queue_btn_open")}
                          >
                            <FolderOpen className="w-3.5 h-3.5" />
                          </Button>

                          <Button
                            variant="ghost"
                            size="icon-sm"
                            onClick={() => removeTask(url)}
                            className="text-muted-foreground hover:text-destructive"
                            title={t("queue_btn_cancel")}
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Bottom Path Banner */}
            <div className="p-4 bg-muted/30 border border-border rounded-xl flex items-center justify-between shrink-0 select-none mt-2">
              <div className="flex items-center gap-2">
                <FolderOpen className="w-4 h-4 text-primary" />
                <span className="font-bold text-xs text-muted-foreground">
                  储存路径: {settings.downloadFolder === "download" ? "Downloads/avdl" : settings.downloadFolder}
                </span>
              </div>
              <span className="text-[11px] text-muted-foreground/60 font-bold hidden sm:inline">
                支持批量全选、单项独立控制与已完成任务归档
              </span>
            </div>
          </div>
        )}
      </main>
    </div>
  );
};
