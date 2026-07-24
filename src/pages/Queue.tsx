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
            // Recover as a standby (paused) task
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

  // Helper to format download speed accurately even on slow networks
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

  // Helper to render status badges
  const renderStatusBadge = (status: string, index: number) => {
    if (status === "pending") {
      return <span className="badge badge-ghost font-bold text-xs select-none">{t("queue_status_pending")}</span>;
    }
    if (status === "downloading") {
      return <span className="badge badge-warning text-white font-bold text-xs select-none">{t("queue_status_downloading")}</span>;
    }
    if (status === "paused") {
      if (index === 0) {
        return <span className="badge badge-outline badge-ghost font-bold text-xs select-none">{t("queue_status_pending")}</span>;
      }
      return <span className="badge badge-warning badge-outline font-bold text-xs select-none">{t("queue_status_paused")}</span>;
    }
    if (status === "merging") {
      return (
        <span className="badge badge-info text-white font-bold text-xs select-none gap-1">
          <Loader2 className="w-3 h-3 animate-spin" /> {t("queue_status_merging")}
        </span>
      );
    }
    if (status === "completed") {
      return <span className="badge badge-success text-white font-bold text-xs select-none">{t("queue_status_completed")}</span>;
    }
    if (status.startsWith("failed")) {
      return <span className="badge badge-error text-white font-bold text-xs select-none" title={status}>{t("queue_status_failed")}</span>;
    }
    return <span className="badge badge-ghost font-bold text-xs select-none">{status}</span>;
  };

  return (
    <div className="flex-1 flex flex-col h-screen overflow-hidden bg-base-100 text-base-content">
      {/* Header */}
      <header className="p-6 border-b border-base-200 bg-base-200/20 flex items-center justify-between shrink-0 select-none">
        <div className="flex items-center gap-3">
          <DownloadCloud className="w-6 h-6 text-error" />
          <h2 className="text-xl font-black">{t("queue_title")}</h2>
        </div>
      </header>

      {/* Main Area */}
      <main className="flex-1 overflow-y-auto p-6 bg-base-100/50">
        {!hasAnyTasks ? (
          // Empty State
          <div className="flex flex-col items-center justify-center h-full text-center select-none max-w-sm mx-auto">
            <DownloadCloud className="w-16 h-16 text-base-content/20 mb-4 animate-bounce" />
            <h3 className="font-extrabold text-lg text-base-content/60">{t("queue_empty")}</h3>
            <p className="text-sm text-base-content/40 mt-1">
              {t("queue_empty_desc")}
            </p>
          </div>
        ) : (
          <div className="max-w-4xl mx-auto space-y-6">
            
            {/* SECTION 1: 进行中 / ACTIVE TASKS */}
            {activeTaskEntries.length > 0 && (
              <div className="space-y-4">
                <div className="flex items-center justify-between select-none">
                  <div className="flex items-center gap-2">
                    <span className="w-2.5 h-2.5 rounded-full bg-error animate-pulse" />
                    <h3 className="font-black text-sm text-base-content/80">
                      {t("queue_section_active")} ({activeTaskEntries.length})
                    </h3>
                  </div>
                </div>

                {/* Batch Action Toolbar */}
                <div className="flex flex-wrap items-center justify-between gap-3 p-3 bg-base-200/50 rounded-xl border border-base-200 shrink-0 select-none shadow-sm">
                  <div className="flex items-center gap-3">
                    <label className="flex items-center gap-2 cursor-pointer font-bold text-xs select-none">
                      <input
                        type="checkbox"
                        checked={allSelected}
                        onChange={toggleSelectAll}
                        className="checkbox checkbox-error checkbox-sm rounded"
                      />
                      <span>{t("browse_btn_select_all")}</span>
                    </label>
                    {selectedQueueUrls.length > 0 && (
                      <span className="badge badge-error text-white font-extrabold text-[11px] rounded-lg">
                        {t("queue_selected_count", {
                          selected: selectedQueueUrls.length,
                          total: activeTaskEntries.length,
                        })}
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-2 flex-wrap">
                    {/* Selected Group */}
                    {selectedQueueUrls.length > 0 && (
                      <div className="join shadow-sm">
                        <button
                          onClick={handleResumeSelected}
                          className="btn btn-xs btn-success text-white join-item font-extrabold gap-1"
                          title={t("queue_btn_start_selected")}
                        >
                          <Play className="w-3 h-3 fill-current" />
                          <span>{t("queue_btn_start_selected")}</span>
                        </button>
                        <button
                          onClick={handlePauseSelected}
                          className="btn btn-xs btn-warning text-white join-item font-extrabold gap-1"
                          title={t("queue_btn_pause_selected")}
                        >
                          <Pause className="w-3 h-3" />
                          <span>{t("queue_btn_pause_selected")}</span>
                        </button>
                        <button
                          onClick={handleCancelSelected}
                          className="btn btn-xs btn-error text-white join-item font-extrabold gap-1"
                          title={t("queue_btn_cancel_selected")}
                        >
                          <Trash2 className="w-3 h-3" />
                          <span>{t("queue_btn_cancel_selected")}</span>
                        </button>
                      </div>
                    )}

                    {/* All Group */}
                    <div className="join shadow-sm">
                      <button
                        onClick={handleResumeAll}
                        className="btn btn-xs btn-outline btn-success join-item font-extrabold gap-1"
                        title={t("queue_btn_start_all")}
                      >
                        <Play className="w-3 h-3 fill-current" />
                        <span>{t("queue_btn_start_all")}</span>
                      </button>
                      <button
                        onClick={handlePauseAll}
                        className="btn btn-xs btn-outline btn-warning join-item font-extrabold gap-1"
                        title={t("queue_btn_pause_all")}
                      >
                        <Pause className="w-3 h-3" />
                        <span>{t("queue_btn_pause_all")}</span>
                      </button>
                      <button
                        onClick={handleCancelAll}
                        className="btn btn-xs btn-outline btn-error join-item font-extrabold gap-1"
                        title={t("queue_btn_cancel_all")}
                      >
                        <Trash2 className="w-3 h-3" />
                        <span>{t("queue_btn_cancel_all")}</span>
                      </button>
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
                        className={`p-3 bg-base-300 rounded-xl flex flex-col gap-2 border transition-all duration-200 ${
                          isSelected
                            ? "border-error bg-error/5 shadow-sm"
                            : isFailed
                            ? "border-error/30 bg-error/5"
                            : "border-base-100 hover:border-base-200 shadow-sm"
                        }`}
                      >
                        {/* Row 1: Checkbox + Title (Truncated) + Buttons */}
                        <div className="flex items-center justify-between gap-3 select-none">
                          <div className="flex items-center gap-2.5 min-w-0 flex-1">
                            <input
                              type="checkbox"
                              checked={isSelected}
                              onChange={() => toggleSelectTask(url)}
                              className="checkbox checkbox-error checkbox-xs rounded shrink-0"
                            />

                            {/* Code + Truncated Title with Rich Tooltip */}
                            <div className="min-w-0 flex-1" title={tooltipText}>
                              <h4 className="font-black text-xs sm:text-sm truncate text-base-content cursor-help hover:text-error transition-colors">
                                {task.title}
                              </h4>
                            </div>
                          </div>

                          {/* Status badge & Single task controls */}
                          <div className="flex items-center gap-1.5 shrink-0 select-none">
                            {renderStatusBadge(task.status, task.index)}

                            {task.status === "downloading" && (
                              <button
                                onClick={() => handlePause(url)}
                                className="btn btn-square btn-ghost btn-xs text-base-content/40 hover:text-warning"
                                title={t("queue_btn_pause")}
                              >
                                <Pause className="w-3.5 h-3.5" />
                              </button>
                            )}

                            {task.status === "paused" && (
                              <button
                                onClick={() => handleResume(url)}
                                className="btn btn-square btn-ghost btn-xs text-base-content/40 hover:text-success"
                                title={t("queue_btn_resume")}
                              >
                                <Play className="w-3.5 h-3.5 fill-current" />
                              </button>
                            )}

                            <button
                              onClick={() => handleCancel(url)}
                              className="btn btn-square btn-ghost btn-xs text-base-content/40 hover:text-error"
                              title={t("queue_btn_cancel")}
                            >
                              <X className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </div>

                        {/* Row 2: Slim Progress Bar + Percentage / Speed */}
                        {!isFailed && (
                          <div className="flex items-center gap-3 w-full pl-6">
                            <div className="flex-1 bg-base-100 rounded-full h-2 overflow-hidden border border-base-200">
                              <div
                                className="bg-gradient-to-r from-error to-pink-500 h-full rounded-full transition-all duration-300"
                                style={{ width: `${percentage}%` }}
                              />
                            </div>
                            <div className="flex items-center gap-2 font-mono text-[11px] font-black shrink-0">
                              <span className="text-base-content/60">{percentage}%</span>
                              {task.status === "downloading" && (
                                <span className="text-error font-bold">{formatSpeed(task.speedKbps)}</span>
                              )}
                            </div>
                          </div>
                        )}

                        {/* Error Banner if Failed */}
                        {isFailed && (
                          <div className="text-xs text-error font-extrabold bg-error/10 p-2 rounded-lg border border-error/20 flex items-center gap-2 mt-1">
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

            {/* SECTION 2: 已完成 / COMPLETED TASKS */}
            {completedTaskEntries.length > 0 && (
              <div className="space-y-4 pt-2">
                <div className="flex items-center justify-between select-none">
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="w-4 h-4 text-success" />
                    <h3 className="font-black text-sm text-base-content/80">
                      {t("queue_section_completed")} ({completedTaskEntries.length})
                    </h3>
                  </div>

                  <button
                    onClick={clearCompletedTasks}
                    className="btn btn-ghost btn-xs text-base-content/40 hover:text-error gap-1 font-bold"
                    title={t("queue_btn_clear_completed")}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                    <span>{t("queue_btn_clear_completed")}</span>
                  </button>
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
                        className="p-3 bg-base-300/60 rounded-xl flex items-center justify-between gap-3 border border-base-100 hover:border-base-200 transition-all shadow-sm"
                      >
                        <div className="flex items-center gap-2.5 min-w-0 flex-1 select-none">
                          <CheckCircle2 className="w-4 h-4 text-success shrink-0" />

                          <div className="min-w-0 flex-1" title={tooltipText}>
                            <h4 className="font-black text-xs sm:text-sm truncate text-base-content/90 hover:text-error cursor-help transition-colors">
                              {task.title}
                            </h4>
                          </div>
                        </div>

                        {/* Right side controls for completed tasks */}
                        <div className="flex items-center gap-2 shrink-0 select-none">
                          <span className="badge badge-success text-white font-bold text-xs">
                            {t("queue_status_completed")}
                          </span>

                          <button
                            onClick={handleOpenFolder}
                            className="btn btn-square btn-ghost btn-xs text-base-content/40 hover:text-info"
                            title={t("queue_btn_open")}
                          >
                            <FolderOpen className="w-3.5 h-3.5" />
                          </button>

                          <button
                            onClick={() => removeTask(url)}
                            className="btn btn-square btn-ghost btn-xs text-base-content/40 hover:text-error"
                            title={t("queue_btn_cancel")}
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Bottom Path Banner */}
            <div className="p-4 bg-base-300/40 border border-base-200 rounded-xl flex items-center justify-between shrink-0 select-none mt-2">
              <div className="flex items-center gap-2">
                <FolderOpen className="w-4 h-4 text-error" />
                <span className="font-bold text-xs text-base-content/75">
                  储存路径: {settings.downloadFolder === "download" ? "Downloads/avdl" : settings.downloadFolder}
                </span>
              </div>
              <span className="text-[11px] text-base-content/40 font-bold hidden sm:inline">
                支持批量全选、单项独立控制与已完成任务归档
              </span>
            </div>
          </div>
        )}
      </main>
    </div>
  );
};
