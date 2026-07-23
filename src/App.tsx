import { useState, useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { Browse } from "./pages/Browse";
import { Queue } from "./pages/Queue";
import { StatsPage } from "./pages/StatsPage";
import { Settings } from "./pages/Settings";
import { useDownloadStore } from "./store/useDownloadStore";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function App() {
  const [activeTab, setActiveTab] = useState<string>("browse");
  const selectedCount = useDownloadStore((state) =>
    Object.values(state.tasks).filter((t) => t.status !== "completed").length
  );
  const settings = useDownloadStore((state) => state.settings);
  const setCfConfig = useDownloadStore((state) => state.setCfConfig);
  const theme = settings.theme;

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme || "dark");
  }, [theme]);

  useEffect(() => {
    // Sync on hydration completion (startup)
    const unsubFinish = useDownloadStore.persist.onFinishHydration((state) => {
      console.log("[Store] Hydration finished. Syncing CF configs:", state.settings.cfConfigs);
      invoke("sync_cf_configs", {
        configs: state.settings.cfConfigs || {},
      }).catch((e) => console.error("Failed to sync CF configs on hydration:", e));
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
    <div className="flex h-screen w-screen overflow-hidden bg-base-100 font-sans">
      {/* Sidebar Navigation */}
      <Sidebar
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        selectedCount={selectedCount}
      />

      {/* Main Pages Router */}
      <div className="flex-1 flex flex-col h-full overflow-hidden">
        {activeTab === "browse" && <Browse />}
        {activeTab === "queue" && <Queue />}
        {activeTab === "stats" && <StatsPage />}

        {activeTab === "settings" && <Settings />}
      </div>
    </div>
  );
}

export default App;
