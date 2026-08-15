import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { UpdateDialog } from "./components/UpdateDialog";
import { Browse } from "./pages/Browse";
import { Queue } from "./pages/Queue";
import { Settings } from "./pages/Settings";
import { StatsPage } from "./pages/StatsPage";
import { useDownloadStore } from "./store/useDownloadStore";
import { useToastStore } from "./store/useToastStore";
import { runUpdateCheck } from "./utils/update";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ToastContainer } from "./components/Toast";

interface ProgressPayload {
  url: string;
  title: string;
  index: number;
  total: number;
  speed_kbps: number;
  status: string;
}

function App() {
  const [activeTab, setActiveTab] = useState<string>("browse");
  const selectedCount = useDownloadStore(
    (state) =>
      Object.values(state.tasks).filter((t) => t.status !== "completed").length,
  );
  const settings = useDownloadStore((state) => state.settings);
  const setCfConfig = useDownloadStore((state) => state.setCfConfig);
  const updateTask = useDownloadStore((state) => state.updateTask);
  const showError = useToastStore((state) => state.showError);
  const theme = settings.theme;

  useEffect(() => {
    if (theme === "light") {
      document.documentElement.classList.remove("dark");
    } else {
      document.documentElement.classList.add("dark");
    }
  }, [theme]);

  // Check for a new release once on startup. The result only lights up the
  // sidebar badge — the update dialog is opened by the user's click, so the
  // check never interrupts the user uninvited.
  useEffect(() => {
    runUpdateCheck();
  }, []);

  // Global download-progress listener. It lives here (not inside the Queue
  // page) so progress/failure events are never missed while the user is on
  // another tab — previously a download that failed or progressed while the
  // user browsed was silently lost and the task looked stuck at 0%.
  useEffect(() => {
    const unlistenPromise = listen<ProgressPayload>(
      "download-progress",
      (event) => {
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
      },
    );

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [updateTask, showError]);

  useEffect(() => {
    // Sync on hydration completion (startup)
    const unsubFinish = useDownloadStore.persist.onFinishHydration((state) => {
      console.log(
        "[Store] Hydration finished. Syncing CF configs:",
        state.settings.cfConfigs,
      );
      invoke("sync_cf_configs", {
        configs: state.settings.cfConfigs || {},
      }).catch((e) =>
        console.error("Failed to sync CF configs on hydration:", e),
      );
    });

    // Sync immediately if already hydrated (like during development or hot reloading)
    if (useDownloadStore.persist.hasHydrated()) {
      invoke("sync_cf_configs", {
        configs: useDownloadStore.getState().settings.cfConfigs || {},
      }).catch((e) => console.error("Failed to sync CF configs initially:", e));
    }

    return () => {
      unsubFinish();
    };
  }, []);

  useEffect(() => {
    // Keep backend in-sync when settings change (verification updates, settings changes, etc.)
    if (useDownloadStore.persist.hasHydrated()) {
      invoke("sync_cf_configs", {
        configs: settings.cfConfigs || {},
      }).catch((e) => console.error("Failed to sync changed CF configs:", e));
    }
  }, [settings.cfConfigs]);

  useEffect(() => {
    const unlistenPromise = listen<{
      domain: string;
      cf_clearance: string;
      user_agent: string;
    }>("cf-verification-success", (event) => {
      const { domain, cf_clearance, user_agent } = event.payload;
      console.log(`[App] Verification succeeded for ${domain}:`, cf_clearance);
      setCfConfig(domain, cf_clearance, user_agent);
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [setCfConfig]);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
      <ToastContainer />
      <UpdateDialog />
      {/* Sidebar Navigation */}
      <Sidebar
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        selectedCount={selectedCount}
      />

      {/* Main Pages Router */}
      <div className="flex-1 flex flex-col h-full overflow-hidden bg-background">
        {activeTab === "browse" && <Browse />}
        {activeTab === "queue" && <Queue />}
        {activeTab === "stats" && <StatsPage />}
        {activeTab === "settings" && <Settings />}
      </div>
    </div>
  );
}

export default App;
