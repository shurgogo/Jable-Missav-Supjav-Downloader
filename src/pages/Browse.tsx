import React, { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Search, Loader2, Compass, CheckSquare, Square, Download, Tag, ShieldCheck, Plus, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "lucide-react";
import { VideoCard, VideoItem } from "../components/VideoCard";
import { useDownloadStore } from "../store/useDownloadStore";
import { useTranslation, translateCategory, translateTagGroup, translateTagName } from "../i18n";

interface Category {
  name: string;
  url: string;
}

interface TagItem {
  name: string;
  slug: string;
  url: string;
}

interface BrowseProps {
  onNavigateToQueue?: () => void;
}

export const Browse: React.FC<BrowseProps> = ({ onNavigateToQueue }) => {
  const { t, language } = useTranslation();
  const [categories, setCategories] = useState<Category[]>([]);
  const [activeUrl, setActiveUrl] = useState<string>("");
  const [activeTagUrl, setActiveTagUrl] = useState<string>("");
  const [videos, setVideos] = useState<VideoItem[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [searchKeyword, setSearchKeyword] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Pagination & Sorting state
  const [page, setPage] = useState<number>(1);
  const [totalPages, setTotalPages] = useState<number>(1);
  const [sortBy, setSortBy] = useState<string>("post_date"); // default to recently updated
  const [sidebarTags, setSidebarTags] = useState<Record<string, TagItem[]>>({});

  const { selectedVideos, toggleSelectVideo, clearSelection, addTask, settings, activeSite, setActiveSite, removeCfConfig } = useDownloadStore();
  const site = activeSite;
  const setSite = setActiveSite;

  const isVerified = useMemo(() => {
    const domainKey = site === "jable" ? "jable" : site === "supjav" ? "supjav" : "missav";
    return Object.entries(settings.cfConfigs || {}).some(
      ([domain, cfg]) =>
        domain.includes(domainKey) &&
        cfg.cfClearance &&
        cfg.cfClearance.trim().length > 0
    );
  }, [settings.cfConfigs, site]);

  // 1. Fetch categories and sidebar tags when site changes
  useEffect(() => {
    const loadInitialData = async () => {
      setLoading(true);
      setError(null);
      setVideos([]);
      setCategories([]);
      setActiveUrl("");
      setActiveTagUrl("");
      setSearchQuery("");
      setSearchKeyword(null);
      setPage(1);
      setTotalPages(1);

      try {
        let finalCats: Category[] = [];

        if (site === "jable") {
          const j_lang = settings.language === "en" ? "en" : settings.language === "ja" ? "jp" : "zh";
          const cats: Category[] = await invoke("get_jable_categories", { lang: j_lang });
          finalCats = cats;
          if (cats.length > 0) {
            setActiveUrl(cats[0].url);
          }

          const tags: Record<string, TagItem[]> = await invoke("get_jable_sidebar_tags");
          setSidebarTags(tags);
          setSortBy("post_date");
        } else if (site === "missav") {
          const m_lang = settings.language === "zh-CN" ? "cn" : settings.language === "en" ? "en" : settings.language === "ja" ? "ja" : "";
          const cats: Category[] = await invoke("get_missav_categories", { lang: m_lang });
          finalCats = cats;
          if (cats.length > 0) {
            setActiveUrl(cats[0].url);
          }
          setSidebarTags({});
          setSortBy("");
        } else {
          // SupJav
          const s_lang = settings.language === "en" ? "" : settings.language === "ja" ? "ja" : "zh";
          const cats: Category[] = await invoke("get_supjav_categories", { lang: s_lang });
          finalCats = cats;
          if (cats.length > 0) {
            setActiveUrl(cats[0].url);
          }
          setSidebarTags({});
          setSortBy("");
        }

        // Translate the category names using t()
        const translatedCats = finalCats.map(cat => ({
          ...cat,
          name: translateCategory(cat.name, settings.language || "zh-TW", t)
        }));
        setCategories(translatedCats);

      } catch (err) {
        console.error("Failed to load categories/tags:", err);
        const errStr = String(err);
        if (errStr.includes("403") || errStr.toLowerCase().includes("forbidden") || errStr.toLowerCase().includes("cloudflare")) {
          console.warn(`[Browse] Categories fetch 403 Forbidden for ${site}, clearing invalid CF config.`);
          removeCfConfig(site);
        }
        setError("無法載入網站分類，請確認您的網路連線或重試防爬驗證。");
      } finally {
        setLoading(false);
      }
    };
    loadInitialData();
  }, [site, settings.language]);

  // 2. Fetch video list when state triggers (activeUrl, searchKeyword, page, sortBy, or site changes)
  useEffect(() => {
    if (!activeUrl && !searchKeyword) return;

    // Guard: Prevent mismatched url requests during tab transitions
    if (activeUrl) {
      if (site === "jable" && !activeUrl.includes("jable.tv")) return;
      if (site === "missav" && !activeUrl.includes("missav")) return;
      if (site === "supjav" && !activeUrl.includes("supjav")) return;
    }

    const loadVideos = async () => {
      setLoading(true);
      setError(null);
      try {
        let response: { videos: VideoItem[]; total_pages: number };

        if (site === "jable") {
          const j_lang = settings.language === "en" ? "en" : settings.language === "ja" ? "jp" : "zh";
          if (searchKeyword) {
            response = await invoke("search_jable", {
              keyword: searchKeyword,
              page,
              sortBy: sortBy || null,
              lang: j_lang,
            });
          } else {
            response = await invoke("fetch_jable_list", {
              url: activeUrl,
              page,
              sortBy: sortBy || null,
              lang: j_lang,
            });
          }
        } else if (site === "missav") {
          if (searchKeyword) {
            const m_lang = settings.language === "zh-CN" ? "cn" : settings.language === "en" ? "en" : settings.language === "ja" ? "ja" : "";
            response = await invoke("search_missav", {
              keyword: searchKeyword,
              page,
              lang: m_lang,
            });
          } else {
            const m_lang = settings.language === "zh-CN" ? "cn" : settings.language === "en" ? "en" : settings.language === "ja" ? "ja" : "";
            response = await invoke("fetch_missav_list", {
              url: activeUrl,
              page,
              lang: m_lang,
            });
          }
        } else {
          // SupJav
          if (searchKeyword) {
            const s_lang = settings.language === "en" ? "" : settings.language === "ja" ? "ja" : "zh";
            response = await invoke("search_supjav", {
              keyword: searchKeyword,
              page,
              lang: s_lang,
            });
          } else {
            const s_lang = settings.language === "en" ? "" : settings.language === "ja" ? "ja" : "zh";
            response = await invoke("fetch_supjav_list", {
              url: activeUrl,
              page,
              lang: s_lang,
            });
          }
        }

        setVideos(response.videos);
        setTotalPages(response.total_pages);
      } catch (err) {
        console.error("Failed to load videos:", err);
        const errStr = String(err);
        if (errStr.includes("403") || errStr.toLowerCase().includes("forbidden") || errStr.toLowerCase().includes("cloudflare") || errStr.includes("遭")) {
          console.warn(`[Browse] Videos fetch 403 Forbidden for ${site}, clearing invalid CF config.`);
          removeCfConfig(site);
        }
        setError(typeof err === "string" ? err : t("browse_verify_error"));
      } finally {
        setLoading(false);
      }
    };

    loadVideos();
  }, [activeUrl, searchKeyword, page, sortBy, site, settings.language]);

  // Cloudflare bypass handler
  const handleVerifyBypass = async () => {
    let targetUrl = "https://supjav.com/";
    if (site === "missav") {
      targetUrl = "https://missav.ws/";
    } else if (site === "jable") {
      targetUrl = "https://jable.tv/";
    }

    try {
      setError(null);
      await invoke("start_cf_verifier", {
        urlStr: targetUrl,
        userAgent: navigator.userAgent,
      });
    } catch (err) {
      console.error("Failed to start Cloudflare verifier:", err);
      setError("無法啟動驗證視窗：" + String(err));
    }
  };

  useEffect(() => {
    const unlistenPromise = listen<{ domain: string }>("cf-verification-success", (event) => {
      const domain = event.payload.domain;
      console.log(`[Browse] Verification success event received for: ${domain}`);
      // Refresh current page if domain matches active site
      if (
        (site === "supjav" && domain.includes("supjav")) ||
        (site === "missav" && domain.includes("missav")) ||
        (site === "jable" && domain.includes("jable"))
      ) {
        // Trigger list reload by forcing a state reset or refetch
        setActiveUrl((prev) => {
          const original = prev;
          setActiveUrl("");
          setTimeout(() => setActiveUrl(original), 50);
          return prev;
        });
        if (searchKeyword) {
          const kw = searchKeyword;
          setSearchKeyword(null);
          setTimeout(() => setSearchKeyword(kw), 50);
        }
      }
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [site, activeUrl, searchKeyword]);

  // 3. Search submit handler
  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) return;
    setActiveUrl("");
    setActiveTagUrl("");
    setSearchKeyword(searchQuery);
    setPage(1);
    setSortBy(""); // Default search sorting to "highest correlation" (empty string)
  };

  const handleCategorySelect = (url: string) => {
    setSearchQuery("");
    setSearchKeyword(null);
    setActiveTagUrl("");
    setActiveUrl(url);
    setPage(1);
    // Keep current sorting preference unless it was search-only (empty string)
    if (site === "jable" && sortBy === "") {
      setSortBy("post_date");
    }
  };

  const handleTagSelect = (url: string) => {
    setSearchQuery("");
    setSearchKeyword(null);
    setActiveTagUrl(url);
    setActiveUrl(url);
    setPage(1);
    // Keep current sorting preference unless it was search-only (empty string)
    if (site === "jable" && sortBy === "") {
      setSortBy("post_date");
    }
  };

  const [jumpPageInput, setJumpPageInput] = useState<string>("");

  const handlePageChange = (newPage: number) => {
    if (newPage < 1 || newPage > totalPages) return;
    setPage(newPage);
    setJumpPageInput("");
  };

  const handleJumpInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    if (val === "" || /^\d+$/.test(val)) {
      setJumpPageInput(val);
    }
  };

  const handleJumpPageSubmit = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!jumpPageInput) return;
    const targetPage = parseInt(jumpPageInput, 10);
    if (isNaN(targetPage)) return;

    let validatedPage = targetPage;
    if (targetPage < 1) {
      validatedPage = 1;
    } else if (targetPage > totalPages) {
      validatedPage = totalPages;
    }

    setJumpPageInput("");
    handlePageChange(validatedPage);
  };

  const handleSortChange = (newSort: string) => {
    setSortBy(newSort);
    setPage(1);
  };

  const handleDoubleClick = async (url: string) => {
    try {
      await openUrl(url);
    } catch (err) {
      console.error("Failed to open URL:", err);
    }
  };

  const allCurrentPageSelected = videos.length > 0 && videos.every((v) => selectedVideos.includes(v.url as string));

  const handleToggleSelectAllCurrentPage = () => {
    if (allCurrentPageSelected) {
      videos.forEach((video) => {
        if (selectedVideos.includes(video.url as string)) {
          toggleSelectVideo(video.url as string);
        }
      });
    } else {
      videos.forEach((video) => {
        if (!selectedVideos.includes(video.url as string)) {
          toggleSelectVideo(video.url as string);
        }
      });
    }
  };

  const handleDownloadSelected = async () => {
    if (selectedVideos.length === 0) return;
    const saveDir = settings.downloadFolder;
    const maxConcurrent = settings.maxConcurrent;
    const resolution = settings.resolution;
    for (const url of selectedVideos) {
      addTask(url);
      try {
        await invoke("start_download", {
          url,
          saveDir,
          maxConcurrent,
          resolution,
        });
      } catch (err) {
        console.error(`Failed to start download for ${url}:`, err);
      }
    }
    clearSelection();
    if (onNavigateToQueue) {
      onNavigateToQueue();
    }
  };

  const handleAddToQueueSelected = () => {
    if (selectedVideos.length === 0) return;
    selectedVideos.forEach((url) => {
      const match = videos.find((v) => v.url === url);
      const title = match ? (match.title as string) : "待解析...";
      addTask(url, title, "paused"); // Add with title and paused status (standby)
    });
    clearSelection();
    if (onNavigateToQueue) {
      onNavigateToQueue();
    }
  };

  return (
    <div className="drawer drawer-end flex-1 h-screen overflow-hidden bg-base-100 text-base-content">
      {/* Hidden toggle checkbox for Drawer */}
      <input id="tag-drawer" type="checkbox" className="drawer-toggle" />

      {/* Drawer main content: Full page layout */}
      <div className="drawer-content flex flex-col h-full overflow-hidden">

        {/* Top Header */}
        <header className="p-6 border-b border-base-200 bg-base-200/20 flex flex-col gap-6 select-none shrink-0">
          <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
            <div className="flex items-center justify-between sm:justify-start gap-4">
              <div className="flex items-center gap-3">
                <Compass className="w-6 h-6 text-error shrink-0" />
                <h2 className="text-xl font-black hidden sm:block">
                  {site === "jable" ? "JableTV" : site === "missav" ? "MissAV" : "SupJav"}
                </h2>
              </div>

              {/* Site Selector tab block */}
              <div className="join bg-base-300 p-0.5 rounded-xl shadow-inner border border-base-200">
                <button
                  onClick={() => setSite("jable")}
                  className={`btn btn-xs rounded-lg font-black px-3 sm:px-4 h-7 min-h-0 border-none transition-all duration-200 ${site === "jable" ? "bg-error text-white shadow-sm" : "btn-ghost text-base-content/60"
                    }`}
                >
                  <span className="hidden sm:inline">JableTV</span>
                  <span className="sm:hidden">J</span>
                </button>
                <button
                  onClick={() => setSite("missav")}
                  className={`btn btn-xs rounded-lg font-black px-3 sm:px-4 h-7 min-h-0 border-none transition-all duration-200 ${site === "missav" ? "bg-error text-white shadow-sm" : "btn-ghost text-base-content/60"
                    }`}
                >
                  <span className="hidden sm:inline">MissAV</span>
                  <span className="sm:hidden">M</span>
                </button>
                <button
                  onClick={() => setSite("supjav")}
                  className={`btn btn-xs rounded-lg font-black px-3 sm:px-4 h-7 min-h-0 border-none transition-all duration-200 ${site === "supjav" ? "bg-error text-white shadow-sm" : "btn-ghost text-base-content/60"
                    }`}
                >
                  <span className="hidden sm:inline">SupJav</span>
                  <span className="sm:hidden">S</span>
                </button>
              </div>

              <button
                onClick={handleVerifyBypass}
                className={`btn btn-xs btn-outline rounded-xl font-bold px-2.5 sm:px-3 h-7 min-h-0 text-[11px] flex items-center gap-1 border-dashed transition-all duration-300 shadow-sm ${
                  isVerified
                    ? "btn-success text-success hover:bg-success hover:text-white"
                    : "btn-error hover:bg-error hover:text-white"
                }`}
                title={t("browse_verify_tooltip")}
              >
                <ShieldCheck className="w-3.5 h-3.5" />
                <span className="hidden sm:inline">
                  {isVerified ? t("settings_cf_verified") : t("settings_cf_verify")}
                </span>
              </button>
            </div>

            {/* Search form */}
            <form onSubmit={handleSearch} className="join w-full lg:max-w-md shadow-md">
              <input
                type="text"
                placeholder={t("browse_search_placeholder")}
                className="input input-bordered join-item w-full font-bold focus:outline-none focus:border-error"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              <button type="submit" className="btn btn-error join-item text-white px-6">
                <Search className="w-5 h-5" />
              </button>
            </form>
          </div>

          {/* Categories, Filters, Sorting & Batch Operations - Compact Single Row */}
          <div className="flex flex-row flex-nowrap items-center justify-between gap-2 w-full select-none">
            <div className="flex flex-row flex-nowrap items-center gap-1.5 shrink-0">
              {/* Primary categories tabs */}
              <div className="tabs tabs-boxed bg-base-300/40 p-0.5 rounded-lg flex flex-row flex-nowrap">
                {categories.slice(0, 3).map((cat) => (
                  <button
                    key={cat.url}
                    onClick={() => handleCategorySelect(cat.url)}
                    className={`tab tab-sm font-black px-3 py-1 rounded-md transition-all duration-200 text-xs ${activeUrl === cat.url && !searchKeyword
                      ? "bg-error text-white shadow-sm"
                      : "text-base-content/75 hover:text-base-content"
                      }`}
                  >
                    {cat.name}
                  </button>
                ))}
              </div>

              {/* Other dynamic categories dropdown */}
              {categories.length > 3 && (
                <div className="dropdown dropdown-bottom">
                  <div tabIndex={0} role="button" className="btn btn-sm btn-outline font-black rounded-lg cursor-pointer px-2.5 text-xs">
                    {t("browse_more_categories")} ({categories.length - 3})
                  </div>
                  <ul tabIndex={0} className="dropdown-content menu p-1.5 shadow-2xl bg-base-200 border border-base-100 rounded-box w-60 max-h-96 overflow-y-auto z-40 mt-1">
                    {categories.slice(3).map((cat) => (
                      <li key={cat.url}>
                        <button
                          onClick={() => handleCategorySelect(cat.url)}
                          className={`font-bold text-xs p-2 ${activeUrl === cat.url && !searchKeyword ? "bg-error text-white" : ""}`}
                        >
                          {cat.name}
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {/* Tag Selector Trigger Label (only for Jable) */}
              {site === "jable" && (
                <label htmlFor="tag-drawer" className="btn btn-sm btn-outline btn-error font-black gap-1.5 rounded-lg cursor-pointer px-2.5 text-xs">
                  <Tag className="w-3.5 h-3.5" />
                  <span>{t("browse_tag_filter")}</span>
                  {activeTagUrl && <span className="text-[10px] bg-error text-white px-1 rounded">{t("browse_tag_active").trim()}</span>}
                </label>
              )}

              {/* Sorting Selection dropdown (only for Jable) */}
              {site === "jable" && (
                <div className="dropdown dropdown-bottom">
                  <div tabIndex={0} role="button" className="btn btn-sm btn-outline btn-error font-black gap-1.5 rounded-lg cursor-pointer px-2.5 text-xs">
                    <span>
                      {t("browse_sort_label")}{
                        sortBy === "post_date_and_popularity" ? t("browse_sort_recent_best") :
                          sortBy === "post_date" ? t("browse_sort_latest") :
                            sortBy === "video_viewed" ? t("browse_sort_most_view") :
                              sortBy === "most_favourited" ? t("browse_sort_most_fav") :
                                searchKeyword ? t("browse_sort_most_rel") : t("browse_sort_latest")
                      }
                    </span>
                  </div>
                  <ul tabIndex={0} className="dropdown-content menu p-1.5 shadow-2xl bg-base-200 border border-base-100 rounded-box w-44 z-40 mt-1">
                    {searchKeyword && (
                      <li>
                        <button
                          onClick={() => handleSortChange("")}
                          className={`font-bold text-xs p-2 ${sortBy === "" ? "bg-error text-white" : ""}`}
                        >
                          {t("browse_sort_most_rel")}
                        </button>
                      </li>
                    )}
                    <li>
                      <button
                        onClick={() => handleSortChange("post_date_and_popularity")}
                        className={`font-bold text-xs p-2 ${sortBy === "post_date_and_popularity" ? "bg-error text-white" : ""}`}
                      >
                        {t("browse_sort_recent_best")}
                      </button>
                    </li>
                    <li>
                      <button
                        onClick={() => handleSortChange("post_date")}
                        className={`font-bold text-xs p-2 ${sortBy === "post_date" ? "bg-error text-white" : ""}`}
                      >
                        {t("browse_sort_latest")}
                      </button>
                    </li>
                    <li>
                      <button
                        onClick={() => handleSortChange("video_viewed")}
                        className={`font-bold text-xs p-2 ${sortBy === "video_viewed" ? "bg-error text-white" : ""}`}
                      >
                        {t("browse_sort_most_view")}
                      </button>
                    </li>
                    <li>
                      <button
                        onClick={() => handleSortChange("most_favourited")}
                        className={`font-bold text-xs p-2 ${sortBy === "most_favourited" ? "bg-error text-white" : ""}`}
                      >
                        {t("browse_sort_most_fav")}
                      </button>
                    </li>
                  </ul>
                </div>
              )}
            </div>

            {/* Batch action operations */}
            {videos.length > 0 && (
              <div className="flex flex-row flex-nowrap items-center gap-1.5 shrink-0">
                <button
                  onClick={handleToggleSelectAllCurrentPage}
                  className="btn btn-sm btn-ghost gap-1.5 font-bold px-2 text-xs"
                  title={allCurrentPageSelected ? "取消全選當前頁面" : "全選當前頁面"}
                >
                  {allCurrentPageSelected ? (
                    <CheckSquare className="w-3.5 h-3.5 text-error" />
                  ) : (
                    <Square className="w-3.5 h-3.5 text-base-content/40" />
                  )}
                  <span className="hidden xs:inline">{t("browse_btn_select_all")}</span>
                </button>
                <button
                  onClick={handleAddToQueueSelected}
                  disabled={selectedVideos.length === 0}
                  className="btn btn-sm btn-outline btn-error gap-1.5 font-bold px-2.5 text-xs disabled:bg-base-300 disabled:text-base-content/30"
                >
                  <Plus className="w-3.5 h-3.5" />
                  <span>{t("browse_btn_queue")}</span>
                  <span className="tabular-nums font-black">({selectedVideos.length})</span>
                </button>
                <button
                  onClick={handleDownloadSelected}
                  disabled={selectedVideos.length === 0}
                  className="btn btn-sm btn-error text-white gap-1.5 font-black px-2.5 text-xs disabled:bg-base-300 disabled:text-base-content/30"
                >
                  <Download className="w-3.5 h-3.5" />
                  <span>{t("browse_btn_download")}</span>
                  <span className="tabular-nums font-black">({selectedVideos.length})</span>
                </button>
              </div>
            )}
          </div>
        </header>

        {/* Video Grid & Pagination container */}
        <main className="flex-1 overflow-y-auto p-6 bg-base-100/50">
          {error && (
            <div className="alert alert-error font-bold shadow-lg max-w-3xl mx-auto my-8 flex flex-col md:flex-row items-center justify-between gap-4">
              <span>{error}</span>
              <button
                onClick={handleVerifyBypass}
                className="btn btn-sm bg-white text-error hover:bg-white/90 border-none rounded-xl shadow font-black gap-1.5 shrink-0"
              >
                <ShieldCheck className="w-4 h-4 animate-bounce" />
                {t("browse_verify_trigger")}
              </button>
            </div>
          )}

          {loading ? (
            // Loading skeleton state
            <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-5">
              {Array.from({ length: 12 }).map((_, idx) => (
                <div key={idx} className="flex flex-col gap-4 w-full">
                  <div className="skeleton aspect-video w-full rounded-xl"></div>
                  <div className="skeleton h-4 w-28"></div>
                  <div className="skeleton h-4 w-full"></div>
                </div>
              ))}
            </div>
          ) : videos.length === 0 ? (
            // Empty State
            <div className="flex flex-col items-center justify-center h-full py-16 text-center select-none animate-fade-in">
              <Loader2 className="w-16 h-16 text-base-content/20 animate-spin mb-4" />
              <h3 className="font-extrabold text-lg text-base-content/60">{t("browse_empty")}</h3>
              <p className="text-sm text-base-content/40 mt-1">{t("browse_empty_desc")}</p>
            </div>
          ) : (
            // Grid List + Pagination
            <div className="flex flex-col h-full justify-between">
              <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-5 animate-fade-in">
                {videos.map((video) => (
                  <VideoCard
                    key={video.url as string}
                    video={video}
                    isSelected={selectedVideos.includes(video.url as string)}
                    onToggle={() => toggleSelectVideo(video.url as string)}
                    onDoubleClick={() => handleDoubleClick(video.url as string)}
                  />
                ))}
              </div>

              {/* Bottom Pagination controls */}
              {totalPages > 1 && (
                <div className="flex items-center justify-center mt-12 py-6 select-none border-t border-base-200/50">
                  <div className="flex flex-col sm:flex-row items-center gap-4 bg-base-200/40 p-2 rounded-2xl border border-base-300 shadow-sm">
                    <div className="flex items-center gap-1">
                      {/* First Page */}
                      <button
                        onClick={() => handlePageChange(1)}
                        disabled={page === 1}
                        className="btn btn-sm btn-circle btn-ghost disabled:opacity-30"
                        title={t("browse_btn_first")}
                      >
                        <ChevronsLeft className="w-4 h-4" />
                      </button>

                      {/* Prev Page */}
                      <button
                        onClick={() => handlePageChange(page - 1)}
                        disabled={page === 1}
                        className="btn btn-sm btn-circle btn-ghost disabled:opacity-30"
                        title={t("browse_btn_prev")}
                      >
                        <ChevronLeft className="w-4 h-4" />
                      </button>

                      <span className="text-xs font-black tracking-wider text-base-content/70 px-4 min-w-[120px] text-center">
                        {t("browse_page_indicator", { page, total: totalPages })}
                      </span>

                      {/* Next Page */}
                      <button
                        onClick={() => handlePageChange(page + 1)}
                        disabled={page === totalPages}
                        className="btn btn-sm btn-circle btn-ghost disabled:opacity-30"
                        title={t("browse_btn_next")}
                      >
                        <ChevronRight className="w-4 h-4" />
                      </button>

                      {/* Last Page */}
                      <button
                        onClick={() => handlePageChange(totalPages)}
                        disabled={page === totalPages}
                        className="btn btn-sm btn-circle btn-ghost disabled:opacity-30"
                        title={t("browse_btn_last")}
                      >
                        <ChevronsRight className="w-4 h-4" />
                      </button>
                    </div>

                    {/* Divider */}
                    <div className="hidden sm:block w-[1px] h-6 bg-base-300 mx-1"></div>

                    {/* Jump Form */}
                    <form onSubmit={handleJumpPageSubmit} className="flex items-center gap-2 pl-1 pr-2">
                      <input
                        type="text"
                        value={jumpPageInput}
                        onChange={handleJumpInputChange}
                        className="input input-sm input-bordered w-12 h-8 rounded-lg text-center font-bold focus:outline-none focus:border-error focus:ring-1 focus:ring-error bg-base-100 p-0"
                      />
                      <button
                        type="submit"
                        disabled={!jumpPageInput}
                        className="btn btn-sm h-8 min-h-8 btn-error text-white rounded-lg font-black px-3.5 transition-all duration-300 hover:shadow-md active:scale-95"
                      >
                        {t("browse_page_jump")}
                      </button>
                    </form>
                  </div>
                </div>
              )}
            </div>
          )}
        </main>
      </div>

      {/* Drawer Sidebar: Slides out from right */}
      <div className="drawer-side z-50">
        <label htmlFor="tag-drawer" aria-label="close sidebar" className="drawer-overlay"></label>
        <div className="p-4 w-80 min-h-full bg-base-200 border-l border-base-300 flex flex-col select-none">
          <div className="flex items-center justify-between pb-4 border-b border-base-300 mb-4 shrink-0">
            <span className="font-black text-sm text-error flex items-center gap-2">
              <Tag className="w-4 h-4" />
              <span>{t("browse_tag_filter")}</span>
            </span>
            {activeTagUrl && (
              <button
                onClick={() => {
                  if (categories.length > 0) {
                    handleCategorySelect(categories[0].url);
                  }
                  const drawerToggle = document.getElementById("tag-drawer") as HTMLInputElement;
                  if (drawerToggle) drawerToggle.checked = false;
                }}
                className="btn btn-xs btn-outline btn-error font-extrabold rounded-lg"
              >
                {t("browse_tag_clear")}
              </button>
            )}
          </div>

          <div className="flex-1 overflow-y-auto space-y-3 pr-1">
            {Object.entries(sidebarTags).map(([group, tags]) => (
              <div key={group} className="collapse collapse-arrow bg-base-300/40 border border-base-200 rounded-xl">
                <input type="checkbox" className="peer" />
                <div className="collapse-title text-xs font-black peer-checked:text-error">
                  {translateTagGroup(group, language)}
                </div>
                <div className="collapse-content flex flex-wrap gap-1.5 pt-2">
                  {tags.map((tag) => (
                    <button
                      key={tag.slug}
                      onClick={() => {
                        handleTagSelect(tag.url);
                        const drawerToggle = document.getElementById("tag-drawer") as HTMLInputElement;
                        if (drawerToggle) drawerToggle.checked = false;
                      }}
                      className={`btn btn-xs rounded-md font-bold transition-all duration-200 ${activeTagUrl === tag.url
                        ? "btn-error text-white shadow-sm"
                        : "btn-ghost text-base-content/75 hover:bg-base-300"
                        }`}
                    >
                      {translateTagName(tag.name, tag.slug, language)}
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};
