import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface TaskInfo {
  site?: Site;
  title: string;
  index: number;
  total: number;
  speedKbps: number;
  status: string; // "pending" | "downloading" | "merging" | "completed" | "failed: ..." | "paused"
}

export interface AppSettings {
  downloadFolder: string;
  maxConcurrent: number;
  resolution: string; // "highest" | "1080" | "720" | "lowest"
  theme: string;
  cfConfigs: Record<string, { cfClearance: string; userAgent: string }>;
  language: string;
  enableLogging?: boolean;
}

export enum Site {
  Jable = "jable",
  Missav = "missav",
  Supjav = "supjav",
}

interface DownloadState {
  selectedVideos: string[];
  tasks: Record<string, TaskInfo>;
  completedTasks: Record<string, TaskInfo>;
  settings: AppSettings;
  activeSite: Site;
  toggleSelectVideo: (url: string) => void;
  clearSelection: () => void;
  addTask: (url: string, title?: string, status?: string, site?: Site) => void;
  updateTask: (url: string, taskUpdate: Partial<TaskInfo>) => void;
  removeTask: (url: string) => void;
  clearCompletedTasks: () => void;
  updateSettings: (settingsUpdate: Partial<AppSettings>) => void;
  setActiveSite: (site: Site) => void;
  setCfConfig: (domain: string, cfClearance: string, userAgent: string) => void;
  removeCfConfig: (domainOrSite: string) => void;
}

export const useDownloadStore = create<DownloadState>()(
  persist(
    (set) => ({
      selectedVideos: [],
      tasks: {},
      completedTasks: {},
      settings: {
        downloadFolder: "download",
        maxConcurrent: 3,
        resolution: "highest",
        theme: "dark",
        cfConfigs: {},
        language: "zh-TW",
        enableLogging: false,
      },
      activeSite: Site.Jable,
      toggleSelectVideo: (url) =>
        set((state) => {
          const isSelected = state.selectedVideos.includes(url);
          const newSelected = isSelected
            ? state.selectedVideos.filter((v) => v !== url)
            : [...state.selectedVideos, url];
          return { selectedVideos: newSelected };
        }),
      clearSelection: () => set({ selectedVideos: [] }),
      addTask: (url, title, status, site) =>
        set((state) => ({
          tasks: {
            ...state.tasks,
            [url]: {
              site: site || state.activeSite,
              title: title || "解析中...",
              index: 0,
              total: 0,
              speedKbps: 0,
              status: status || "pending",
            },
          },
        })),
      updateTask: (url, taskUpdate) =>
        set((state) => {
          // A task that already completed must not be resurrected by a stale
          // event from an older download instance (pause → resume race where
          // the old task's "paused"/"failed" event arrives after the new
          // instance already finished).
          if (
            taskUpdate.status !== "completed" &&
            state.completedTasks[url] &&
            !state.tasks[url]
          ) {
            return state;
          }
          const existing = state.tasks[url] || state.completedTasks[url] || {
            title: "解析中...",
            index: 0,
            total: 0,
            speedKbps: 0,
            status: "pending",
          };
          const updatedTask = { ...existing, ...taskUpdate };
          const isCompleted = updatedTask.status === "completed";

          const newTasks = { ...state.tasks };
          if (isCompleted) {
            delete newTasks[url];
          } else {
            newTasks[url] = updatedTask;
          }

          return {
            tasks: newTasks,
            completedTasks: isCompleted
              ? { ...state.completedTasks, [url]: updatedTask }
              : state.completedTasks,
          };
        }),
      removeTask: (url) =>
        set((state) => {
          const newTasks = { ...state.tasks };
          const newCompleted = { ...state.completedTasks };
          delete newTasks[url];
          delete newCompleted[url];
          return { tasks: newTasks, completedTasks: newCompleted };
        }),
      clearCompletedTasks: () => set({ completedTasks: {} }),
      updateSettings: (settingsUpdate) =>
        set((state) => ({
          settings: { ...state.settings, ...settingsUpdate },
        })),
      setActiveSite: (site) => set({ activeSite: site }),
      setCfConfig: (domain, cfClearance, userAgent) =>
        set((state) => ({
          settings: {
            ...state.settings,
            cfConfigs: {
              ...(state.settings.cfConfigs || {}),
              [domain]: { cfClearance, userAgent },
            },
          },
        })),
      removeCfConfig: (domainOrSite) =>
        set((state) => {
          const newConfigs = { ...(state.settings.cfConfigs || {}) };
          Object.keys(newConfigs).forEach((domain) => {
            if (domain.includes(domainOrSite)) {
              delete newConfigs[domain];
            }
          });
          return {
            settings: {
              ...state.settings,
              cfConfigs: newConfigs,
            },
          };
        }),
    }),
    {
      name: "avdl-download-store",
      // Persist the settings configuration, activeSite, and completedTasks history
      partialize: (state) => ({
        settings: state.settings,
        activeSite: state.activeSite,
        completedTasks: state.completedTasks || {},
      }),
    }
  )
);
