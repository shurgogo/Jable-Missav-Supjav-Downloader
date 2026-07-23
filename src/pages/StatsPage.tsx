import React, { useEffect, useState, useMemo } from "react";
import { BarChart3, Database, Trophy, ShieldCheck, Loader2 } from "lucide-react";
import { useDownloadStore } from "../store/useDownloadStore";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";

export const StatsPage: React.FC = () => {
  const { t } = useTranslation();
  const tasks = useDownloadStore((state) => state.tasks);
  const completedTasksHistory = useDownloadStore((state) => state.completedTasks || {});
  const settings = useDownloadStore((state) => state.settings);

  const [folderSize, setFolderSize] = useState<number>(0);
  const [loadingSize, setLoadingSize] = useState<boolean>(true);
  const [diskSpace, setDiskSpace] = useState<{ total_space: number; available_space: number } | null>(null);
  const [loadingDisk, setLoadingDisk] = useState<boolean>(true);

  // Combine historical completed tasks with active tasks
  const allTasks = useMemo(() => {
    return { ...completedTasksHistory, ...tasks };
  }, [completedTasksHistory, tasks]);

  const totalTasks = Object.keys(allTasks).length;
  const completedTasks = Object.values(allTasks).filter((t) => t.status === "completed").length;
  const completionRate = totalTasks > 0 ? Math.round((completedTasks / totalTasks) * 100) : 0;

  useEffect(() => {
    const fetchSize = async () => {
      setLoadingSize(true);
      try {
        const size: number = await invoke("get_folder_size", {
          saveDir: settings.downloadFolder,
        });
        setFolderSize(size);
      } catch (err) {
        console.error("Failed to get folder size:", err);
      } finally {
        setLoadingSize(false);
      }
    };
    fetchSize();
  }, [settings.downloadFolder, completedTasks]);

  useEffect(() => {
    const fetchDiskSpace = async () => {
      setLoadingDisk(true);
      try {
        const info: { total_space: number; available_space: number } = await invoke("get_disk_space_info", {
          saveDir: settings.downloadFolder,
        });
        setDiskSpace(info);
      } catch (err) {
        console.error("Failed to get disk space info:", err);
      } finally {
        setLoadingDisk(false);
      }
    };
    fetchDiskSpace();
  }, [settings.downloadFolder, completedTasks]);

  const diskStats = useMemo(() => {
    if (!diskSpace || folderSize === null) return null;
    const total = diskSpace.total_space;
    const free = diskSpace.available_space;
    const used = total - free;
    const appUsed = folderSize;
    const otherUsed = Math.max(0, used - appUsed);

    const appPercent = total > 0 ? (appUsed / total) * 100 : 0;
    const otherPercent = total > 0 ? (otherUsed / total) * 100 : 0;
    const freePercent = total > 0 ? (free / total) * 100 : 0;

    return {
      total,
      free,
      used,
      appUsed,
      otherUsed,
      appPercent: parseFloat(appPercent.toFixed(2)),
      otherPercent: parseFloat(otherPercent.toFixed(2)),
      freePercent: parseFloat(freePercent.toFixed(2)),
      ratioStr: `${appPercent.toFixed(2)}%`,
      freeRatioStr: `${freePercent.toFixed(2)}%`
    };
  }, [diskSpace, folderSize]);

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
  };

  const COMMON_TAGS = [
    "中文字幕", "無修正", "巨乳", "人妻", "有碼", "無碼", "美乳", "美臀", 
    "黑絲", "美腿", "制服", "學生", "OL", "教師", "醫生", "護士", "空姐",
    "女僕", "JK", "蘿莉", "癡漢", "強姦", "群交", "熟女", "絲襪", "自拍",
    "Cosplay", "中出", "顏射", "口交", "潮吹", "按摩", "泡泡浴", "催眠",
    "三上悠亞", "河北彩花", "深田詠美", "大槻響", "新有菜", "櫻空桃"
  ];

  const wordCloudTags = useMemo(() => {
    const counts: Record<string, number> = {};
    const titles = Object.values(tasks).map((t) => t.title);
    
    titles.forEach((title) => {
      if (!title || title === "解析中...") return;
      
      COMMON_TAGS.forEach((keyword) => {
        if (title.includes(keyword)) {
          counts[keyword] = (counts[keyword] || 0) + 1;
        }
      });
      
      const matches = title.match(/[\[\({]([^\]\)}]+)[\]\)}]/g);
      if (matches) {
        matches.forEach((m) => {
          const clean = m.replace(/[\[\(\{\]\)\}]/g, "").trim();
          if (
            clean.length >= 2 && 
            clean.length <= 6 && 
            !clean.includes("1080") && 
            !clean.includes("720") && 
            !clean.includes("hls") &&
            !clean.includes("MP4")
          ) {
            counts[clean] = (counts[clean] || 0) + 1;
          }
        });
      }
    });

    const list = Object.entries(counts)
      .map(([text, value]) => ({ text, value }))
      .sort((a, b) => b.value - a.value);

    const topTags = list.slice(0, 35);
    return topTags.sort((a, b) => (a.text.charCodeAt(0) % 7) - (b.text.charCodeAt(0) % 7));
  }, [tasks]);

  return (
    <div className="flex-1 flex flex-col h-screen overflow-hidden bg-base-100 text-base-content select-none">
      <style>{`
        .word-cloud-tag {
          transition: transform 0.25s cubic-bezier(0.175, 0.885, 0.32, 1.275), filter 0.2s, opacity 0.2s;
        }
        .word-cloud-tag:hover {
          transform: scale(1.3) translateY(-5px) rotate(3deg);
          filter: brightness(1.25);
          z-index: 10;
        }
      `}</style>

      {/* Header */}
      <header className="p-6 border-b border-base-200 bg-base-200/20 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <BarChart3 className="w-6 h-6 text-error" />
          <h2 className="text-xl font-black">{t("stats_title")}</h2>
        </div>
      </header>

      {/* Main Container */}
      <main className="flex-1 overflow-y-auto p-6 bg-base-100/50">
        <div className="max-w-4xl mx-auto space-y-8">
          
          {/* Stats section */}
          <div className="stats stats-vertical lg:stats-horizontal shadow bg-base-300 w-full rounded-2xl border border-base-100">
            <div className="stat">
              <div className="stat-figure text-error">
                <Trophy className="w-8 h-8" />
              </div>
              <div className="stat-title text-base-content/50 font-bold text-xs">{t("stats_total")}</div>
              <div className="stat-value text-error font-black text-3xl mt-1">{totalTasks}</div>
              <div className="stat-desc font-bold text-success mt-1">
                {t("stats_total_desc", { count: completedTasks })}
              </div>
            </div>
            
            <div className="stat">
              <div className="stat-figure text-pink-500">
                <Database className="w-8 h-8" />
              </div>
              <div className="stat-title text-base-content/50 font-bold text-xs">{t("stats_storage")}</div>
              <div className="stat-value text-pink-500 font-black text-3xl mt-1">
                {loadingSize ? (
                  <span className="flex items-center gap-2 text-lg text-base-content/40 font-bold">
                    <Loader2 className="w-4 h-4 animate-spin text-pink-500" /> {t("stats_loading")}
                  </span>
                ) : (
                  formatBytes(folderSize)
                )}
              </div>
              <div className="stat-desc font-bold text-base-content/40 mt-1 truncate max-w-[240px]" title={settings.downloadFolder}>
                {t("stats_folder_path", { path: settings.downloadFolder === "download" ? "Downloads/avdl" : settings.downloadFolder })}
              </div>
            </div>
            
            <div className="stat">
              <div className="stat-figure text-info">
                <ShieldCheck className="w-8 h-8" />
              </div>
              <div className="stat-title text-base-content/50 font-bold text-xs">{t("stats_rate")}</div>
              <div className="stat-value text-info font-black text-3xl mt-1">{completionRate}%</div>
              <div className="stat-desc font-bold text-base-content/40 mt-1">
                {t("stats_rate_desc", { count: totalTasks - completedTasks })}
              </div>
            </div>
          </div>

          {/* Storage & Disk Status Card */}
          <div className="card bg-base-300 border border-base-100 shadow-xl rounded-2xl">
            <div className="card-body p-6 gap-6">
              <h3 className="card-title text-base font-extrabold border-b border-base-100 pb-3 text-pink-500 flex items-center gap-2">
                <Database className="w-5 h-5" /> {t("stats_health_title")}
              </h3>

              {loadingSize || loadingDisk || !diskStats ? (
                <div className="flex items-center justify-center py-8 gap-2">
                  <Loader2 className="w-5 h-5 animate-spin text-pink-500" />
                  <span className="text-sm font-bold text-base-content/40">{t("stats_loading")}</span>
                </div>
              ) : (
                <div className="space-y-6">
                  {/* Progress Bar */}
                  <div className="space-y-2">
                    <div className="w-full bg-base-200/50 h-5 rounded-full overflow-hidden flex border border-base-100">
                      {diskStats.appPercent > 0 && (
                        <div 
                          style={{ width: `${diskStats.appPercent}%` }} 
                          className="bg-pink-500 h-full transition-all duration-500"
                          title={`${t("stats_bar_app")}: ${formatBytes(diskStats.appUsed)} (${diskStats.appPercent}%)`}
                        />
                      )}
                      {diskStats.otherPercent > 0 && (
                        <div 
                          style={{ width: `${diskStats.otherPercent}%` }} 
                          className="bg-neutral-content/40 dark:bg-neutral h-full transition-all duration-500"
                          title={`${t("stats_bar_other")}: ${formatBytes(diskStats.otherUsed)} (${diskStats.otherPercent}%)`}
                        />
                      )}
                      {diskStats.freePercent > 0 && (
                        <div 
                          style={{ width: `${diskStats.freePercent}%` }} 
                          className="bg-success h-full transition-all duration-500"
                          title={`${t("stats_bar_free")}: ${formatBytes(diskStats.free)} (${diskStats.freePercent}%)`}
                        />
                      )}
                    </div>
                    
                    {/* Legend */}
                    <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-xs font-bold text-base-content/70">
                      <div className="flex items-center gap-1.5">
                        <span className="w-3 h-3 rounded-full bg-pink-500"></span>
                        <span>{t("stats_bar_app")}: {formatBytes(diskStats.appUsed)} ({diskStats.appPercent}%)</span>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="w-3 h-3 rounded-full bg-neutral-content/40 dark:bg-neutral"></span>
                        <span>{t("stats_bar_other")}: {formatBytes(diskStats.otherUsed)} ({diskStats.otherPercent}%)</span>
                      </div>
                      <div className="flex items-center gap-1.5">
                        <span className="w-3 h-3 rounded-full bg-success"></span>
                        <span>{t("stats_bar_free")}: {formatBytes(diskStats.free)} ({diskStats.freeRatioStr})</span>
                      </div>
                    </div>
                  </div>

                  {/* Detailed Disk Specs Grid */}
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 pt-2">
                    <div className="bg-base-200/40 p-4 rounded-xl border border-base-100 flex flex-col gap-1">
                      <span className="text-[10px] text-base-content/40 font-extrabold uppercase">{t("stats_total_disk")}</span>
                      <span className="text-lg font-black text-base-content/80">{formatBytes(diskStats.total)}</span>
                    </div>
                    <div className="bg-base-200/40 p-4 rounded-xl border border-base-100 flex flex-col gap-1">
                      <span className="text-[10px] text-base-content/40 font-extrabold uppercase">{t("stats_ratio")}</span>
                      <span className="text-lg font-black text-pink-500">{diskStats.ratioStr}</span>
                    </div>
                    <div className="bg-base-200/40 p-4 rounded-xl border border-base-100 flex flex-col gap-1">
                      <span className="text-[10px] text-base-content/40 font-extrabold uppercase">{t("stats_used")}</span>
                      <span className="text-lg font-black text-base-content/80">{formatBytes(diskStats.used)}</span>
                    </div>
                    <div className="bg-base-200/40 p-4 rounded-xl border border-base-100 flex flex-col gap-1">
                      <span className="text-[10px] text-base-content/40 font-extrabold uppercase">{t("stats_free")}</span>
                      <span className="text-lg font-black text-success">{formatBytes(diskStats.free)}</span>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Preference Word Cloud */}
          <div className="card bg-base-300 border border-base-100 shadow-xl rounded-2xl">
            <div className="card-body p-6 gap-6">
              <h3 className="card-title text-base font-extrabold border-b border-base-100 pb-3 text-error">
                {t("stats_cloud_title")}
              </h3>
              
              {wordCloudTags.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-12 text-center">
                  <BarChart3 className="w-12 h-12 text-base-content/25 mb-3" />
                  <p className="text-sm text-base-content/50 font-bold">
                    {t("stats_cloud_empty")}
                  </p>
                  <p className="text-xs text-base-content/40 mt-1">
                    {t("stats_cloud_empty_desc")}
                  </p>
                </div>
              ) : (
                <div className="flex flex-wrap items-center justify-center gap-4 py-8 px-4 min-h-[300px] bg-base-100/30 rounded-2xl border border-base-100/50">
                  {wordCloudTags.map((tag, idx) => {
                    const maxCount = Math.max(...wordCloudTags.map(t => t.value));
                    const size = 12 + 22 * (tag.value / (maxCount || 1));
                    const opacity = 0.6 + 0.4 * (tag.value / (maxCount || 1));
                    
                    const colorClasses = [
                      "text-error", "text-pink-500", "text-info", 
                      "text-success", "text-warning", "text-primary",
                      "text-secondary", "text-accent"
                    ];
                    const color = colorClasses[idx % colorClasses.length];

                    return (
                      <span
                        key={tag.text}
                        style={{ 
                          fontSize: `${size}px`, 
                          opacity: opacity,
                          display: "inline-block"
                        }}
                        className={`word-cloud-tag font-black ${color} px-3.5 py-1.5 rounded-2xl bg-base-300 hover:bg-base-200 border border-base-100/60 cursor-pointer shadow-md hover:shadow-lg`}
                        title={`下載了 ${tag.value} 次`}
                      >
                        {tag.text}
                      </span>
                    );
                  })}
                </div>
              )}
            </div>
          </div>

        </div>
      </main>
    </div>
  );
};
