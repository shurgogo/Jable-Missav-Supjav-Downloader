import React, { useState, useEffect, useRef } from "react";
import { Settings as SettingsIcon, Info, ShieldCheck, CheckCircle2, FolderOpen, ExternalLink } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useDownloadStore } from "../store/useDownloadStore";
import { useToastStore } from "../store/useToastStore";
import { useTranslation } from "../i18n";

export const Settings: React.FC = () => {
  const { t } = useTranslation();
  const showError = useToastStore((state) => state.showError);
  const settings = useDownloadStore((state) => state.settings);
  const updateSettings = useDownloadStore((state) => state.updateSettings);

  const [showToast, setShowToast] = useState<boolean>(false);
  const toastTimeoutRef = useRef<any>(null);

  // Clean up timer on unmount
  useEffect(() => {
    return () => {
      if (toastTimeoutRef.current) {
        clearTimeout(toastTimeoutRef.current);
      }
    };
  }, []);

  const triggerAutoSaveToast = () => {
    if (toastTimeoutRef.current) {
      clearTimeout(toastTimeoutRef.current);
    }
    setShowToast(true);
    toastTimeoutRef.current = setTimeout(() => {
      setShowToast(false);
    }, 1500);
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
    <div className="flex-1 flex flex-col h-screen overflow-hidden bg-base-100 text-base-content relative">

      {/* Toast Alert */}
      {showToast && (
        <div className="toast toast-top toast-center z-50">
          <div className="alert alert-success shadow-lg flex items-center gap-2 font-bold text-white">
            <CheckCircle2 className="w-5 h-5" />
            <span>{t("settings_autosave_toast")}</span>
          </div>
        </div>
      )}

      {/* Header */}
      <header className="p-6 border-b border-base-200 bg-base-200/20 flex items-center justify-between shrink-0 select-none">
        <div className="flex items-center gap-3">
          <SettingsIcon className="w-6 h-6 text-error" />
          <h2 className="text-xl font-black">{t("settings_title")}</h2>
        </div>
        <div className="badge badge-success text-white font-extrabold px-3.5 py-2.5 rounded-xl flex items-center gap-1.5 shadow-sm shadow-success/15 animate-pulse">
          <span className="w-1.5 h-1.5 rounded-full bg-white"></span>
          {t("settings_autosave_badge")}
        </div>
      </header>

      {/* Settings Form Grid */}
      <main className="flex-1 overflow-y-auto p-6">
        <div className="max-w-3xl mx-auto space-y-8">
          {/* Card: Download Preferences */}
          <div className="card bg-base-300 border border-base-100 shadow-xl">
            <div className="card-body gap-6">
              <h3 className="card-title text-base font-extrabold border-b border-base-100 pb-2 text-error">
                {t("nav_settings")}
              </h3>

              {/* Setting row: folder */}
              <div className="form-control w-full">
                <label className="label">
                  <span className="label-text font-black">{t("settings_folder")}</span>
                </label>
                <div className="join w-full shadow-sm">
                  <input
                    type="text"
                    value={settings.downloadFolder}
                    onChange={(e) => {
                      updateSettings({ downloadFolder: e.target.value });
                      triggerAutoSaveToast();
                    }}
                    className="input input-bordered join-item w-full font-bold focus:outline-none focus:border-error"
                    placeholder="例如: download 或 /Users/username/Downloads/my_videos"
                  />
                  <button
                    onClick={handleBrowse}
                    className="btn btn-error text-white join-item font-bold px-4 gap-2 shrink-0 shadow-sm hover:brightness-110 active:scale-95 transition-all"
                  >
                    <FolderOpen className="w-4 h-4" />
                    {t("settings_folder_browse")}
                  </button>
                  <button
                    onClick={handleOpenFolder}
                    className="btn bg-base-200 hover:bg-base-100 text-base-content join-item font-bold px-4 gap-2 shrink-0 active:scale-95 transition-all"
                  >
                    <ExternalLink className="w-4 h-4 text-base-content/70" />
                    {t("settings_folder_open")}
                  </button>
                </div>
                <span className="label-text-alt mt-1.5 text-base-content/40 font-semibold">
                  {t("settings_folder_desc")}
                </span>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                {/* Setting: concurrent */}
                <div className="form-control w-full">
                  <label className="label">
                    <span className="label-text font-black">{t("settings_concurrent")}</span>
                  </label>
                  <select
                    className="select select-bordered font-bold focus:outline-none"
                    value={settings.maxConcurrent}
                    onChange={(e) => {
                      updateSettings({ maxConcurrent: Number(e.target.value) });
                      triggerAutoSaveToast();
                    }}
                  >
                    {[1, 2, 3, 4, 5, 6, 8, 10, 12, 16].map((num) => (
                      <option key={num} value={num}>
                        {num}
                      </option>
                    ))}
                  </select>
                  <span className="label-text-alt mt-1 text-base-content/40 font-semibold">
                    {t("settings_concurrent_desc")}
                  </span>
                </div>

                {/* Setting: resolution */}
                <div className="form-control w-full">
                  <label className="label">
                    <span className="label-text font-black">{t("settings_resolution")}</span>
                  </label>
                  <select
                    className="select select-bordered font-bold focus:outline-none"
                    value={settings.resolution}
                    onChange={(e) => {
                      updateSettings({ resolution: e.target.value });
                      triggerAutoSaveToast();
                    }}
                  >
                    <option value="highest">Highest</option>
                    <option value="1080">1080P</option>
                    <option value="720">720P</option>
                    <option value="lowest">Lowest</option>
                  </select>
                  <span className="label-text-alt mt-1 text-base-content/40 font-semibold">
                    {t("settings_resolution_desc")}
                  </span>
                </div>
              </div>

              {/* Setting: Theme selection */}
              <div className="form-control w-full border-t border-base-100 pt-4">
                <label className="label">
                  <span className="label-text font-black text-base-content/85">{t("settings_theme")}</span>
                </label>
                <select
                  className="select select-bordered font-bold focus:outline-none"
                  value={settings.theme || "dark"}
                  onChange={(e) => {
                    const newTheme = e.target.value;
                    updateSettings({ theme: newTheme });
                    triggerAutoSaveToast();
                  }}
                >
                  <option value="light">☀️ Light (明亮)</option>
                  <option value="dark">🌙 Dark (暗黑)</option>
                  <option value="cupcake">🧁 Cupcake (馬卡龍)</option>
                  <option value="bumblebee">🐝 Bumblebee (大黃蜂)</option>
                  <option value="emerald">🟢 Emerald (翡翠)</option>
                  <option value="corporate">🏢 Corporate (商務藍)</option>
                  <option value="synthwave">🔮 Synthwave (霓虹紫)</option>
                  <option value="retro">📻 Retro (復古)</option>
                  <option value="cyberpunk">💛 Cyberpunk (賽博朋克)</option>
                  <option value="valentine">💗 Valentine (情人節)</option>
                  <option value="halloween">🎃 Halloween (萬聖節)</option>
                  <option value="garden">🌸 Garden (花園)</option>
                  <option value="forest">🌲 Forest (森林)</option>
                  <option value="aqua">🌊 Aqua (深海藍)</option>
                  <option value="lofi">🎹 Lofi (簡約黑白)</option>
                  <option value="pastel">🎨 Pastel (柔和馬卡龍)</option>
                  <option value="fantasy">🦄 Fantasy (夢幻)</option>
                  <option value="wireframe">📐 Wireframe (線框)</option>
                  <option value="black">🖤 Black (純黑)</option>
                  <option value="luxury">💎 Luxury (奢華金)</option>
                  <option value="dracula">🧛 Dracula (吸血鬼)</option>
                  <option value="cmyk">🖨️ CMYK (印刷色)</option>
                  <option value="autumn">🍁 Autumn (秋天)</option>
                  <option value="business">💼 Business (沉穩商務)</option>
                  <option value="acid">🧪 Acid (螢光黃)</option>
                  <option value="lemonade">🍋 Lemonade (檸檬水)</option>
                  <option value="night">🌃 Night (深夜藍)</option>
                  <option value="coffee">☕ Coffee (咖啡)</option>
                  <option value="winter">❄️ Winter (冰雪藍)</option>
                  <option value="dim">🌁 Dim (暮光灰)</option>
                  <option value="nord">❄️ Nord (極地)</option>
                  <option value="sunset">🌅 Sunset (日落)</option>
                </select>
                <span className="label-text-alt mt-1 text-base-content/40 font-semibold">
                  {t("settings_theme_desc")}
                </span>
              </div>

              {/* Setting: Language selection */}
              <div className="form-control w-full border-t border-base-100 pt-4">
                <label className="label">
                  <span className="label-text font-black text-base-content/85">{t("settings_lang")}</span>
                </label>
                <select
                  className="select select-bordered font-bold focus:outline-none"
                  value={settings.language || "zh-TW"}
                  onChange={(e) => {
                    const newLang = e.target.value;
                    updateSettings({ language: newLang });
                    triggerAutoSaveToast();
                  }}
                >
                  <option value="zh-TW">繁體中文 (Traditional Chinese)</option>
                  <option value="zh-CN">简体中文 (Simplified Chinese)</option>
                  <option value="en">English (English)</option>
                  <option value="ja">日本語 (Japanese)</option>
                </select>
                <span className="label-text-alt mt-1.5 text-base-content/40 font-semibold">
                  {t("settings_lang_desc")}
                </span>
              </div>

            </div>
          </div>

          {/* Card: Cloudflare Bypass Check */}
          <div className="card bg-base-300 border border-base-100 shadow-xl rounded-2xl">
            <div className="card-body gap-4">
              <h3 className="card-title text-base font-extrabold border-b border-base-100 pb-2 text-error flex items-center gap-2">
                <ShieldCheck className="w-5 h-5" />
                {t("settings_cf")}
              </h3>

              <p className="text-xs text-base-content/60 font-semibold">
                {t("settings_cf_desc")}
              </p>

              <div className="overflow-x-auto w-full border border-base-100 rounded-xl mt-2 bg-base-200/30">
                <table className="table table-sm w-full font-bold">
                  <thead>
                    <tr className="border-b border-base-100 text-base-content/60">
                      <th>Site</th>
                      <th>Domain</th>
                      <th>Token Status</th>
                      <th>Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {[
                      { name: "JableTV", domain: "jable.tv", url: "https://jable.tv/" },
                      { name: "MissAV", domain: "missav.ws", url: "https://missav.ws/" },
                      { name: "SupJav", domain: "supjav.com", url: "https://supjav.com/" },
                    ].map((site) => {
                      const cfg = settings.cfConfigs?.[site.domain];
                      const hasCookie = !!cfg?.cfClearance;
                      return (
                        <tr key={site.domain} className="border-b border-base-100/50">
                          <td className="font-extrabold text-sm">{site.name}</td>
                          <td className="text-xs font-mono">{site.domain}</td>
                          <td>
                            {hasCookie ? (
                              <span className="badge badge-success text-white badge-sm font-semibold truncate max-w-[150px]" title={cfg.cfClearance}>
                                Authorized ({cfg.cfClearance.substring(0, 10)}...)
                              </span>
                            ) : (
                              <span className="badge badge-ghost text-base-content/40 badge-sm font-semibold">Unauthorized</span>
                            )}
                          </td>
                          <td>
                            <div className="flex gap-2">
                              <button
                                onClick={() => handleAutoVerify(site.url)}
                                className="btn btn-xs btn-error text-white font-extrabold rounded-lg"
                              >
                                {t("settings_cf_verify")}
                              </button>
                              {hasCookie && (
                                <button
                                  onClick={() => handleClearVerify(site.domain)}
                                  className="btn btn-xs btn-outline btn-ghost font-bold rounded-lg"
                                >
                                  {t("settings_cf_clear")}
                                </button>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>

              <div className="flex items-start gap-3 bg-base-200/50 p-4 rounded-xl border border-base-100 mt-2">
                <Info className="w-5 h-5 text-error shrink-0 mt-0.5" />
                <div className="text-xs text-base-content/60 font-semibold space-y-1">
                  <p>Client engine emulates Chrome 120 client fingerprints to bypass Cloudflare protection.</p>
                  <p>Click "Verify" to open target check window, complete Turnstile challenge inside the window. The window closes automatically once verification is successful.</p>
                </div>
              </div>
            </div>
          </div>

        </div>
      </main>
    </div>
  );
};
