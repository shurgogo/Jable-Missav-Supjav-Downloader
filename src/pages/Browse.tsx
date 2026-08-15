import React, { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Search,
  Loader2,
  Compass,
  CheckSquare,
  Square,
  Download,
  Tag,
  ShieldCheck,
  Plus,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ChevronDown,
} from "lucide-react";
import { VideoCard, VideoItem } from "../components/VideoCard";
import { useDownloadStore, Site } from "../store/useDownloadStore";
import { useToastStore } from "../store/useToastStore";
import { parseAppError } from "../utils/error";
import { useTranslation, translateCategory, translateTagGroup, translateTagName } from "../i18n";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from "../components/ui/sheet";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../components/ui/dropdown-menu";
import { cn } from "../lib/utils";

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
  const showError = useToastStore((state) => state.showError);
  const showSuccess = useToastStore((state) => state.showSuccess);
  const [categories, setCategories] = useState<Category[]>([]);
  const [activeUrl, setActiveUrl] = useState<string>("");
  const [activeTagUrl, setActiveTagUrl] = useState<string>("");
  const [videos, setVideos] = useState<VideoItem[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [searchKeyword, setSearchKeyword] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isSheetOpen, setIsSheetOpen] = useState<boolean>(false);

  // Pagination & Sorting state
  const [page, setPage] = useState<number>(1);
  const [totalPages, setTotalPages] = useState<number>(1);
  const [sortBy, setSortBy] = useState<string>("post_date"); // default to recently updated
  const [sidebarTags, setSidebarTags] = useState<Record<string, TagItem[]>>({});

  const { selectedVideos, toggleSelectVideo, clearSelection, addTask, tasks, settings, activeSite, setActiveSite, removeCfConfig } = useDownloadStore();
  const site = activeSite;
  const setSite = setActiveSite;

  const isVerified = useMemo(() => {
    const domainKey = site;
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
        const getLang = () => {
          if (site === Site.Jable) return settings.language === "en" ? "en" : settings.language === "ja" ? "jp" : "zh";
          if (site === Site.Missav) return settings.language === "zh-CN" ? "cn" : settings.language === "en" ? "en" : settings.language === "ja" ? "ja" : "";
          return settings.language === "en" ? "" : settings.language === "ja" ? "ja" : "zh";
        };
        const lang = getLang();

        const cats: Category[] = await invoke("get_categories", { req: { site, lang } });
        finalCats = cats;
        if (cats.length > 0) {
          setActiveUrl(cats[0].url);
        }

        const tags: Record<string, TagItem[]> = await invoke("get_sidebar_tags", { req: { site } });
        setSidebarTags(tags);
        setSortBy(site === Site.Jable ? "post_date" : "");

        const translatedCats = finalCats.map((cat: Category) => ({
          ...cat,
          name: translateCategory(cat.name, settings.language || "zh-TW", t)
        }));
        setCategories(translatedCats);

      } catch (err) {
        console.error("Failed to load categories/tags:", err);
        const parsed = parseAppError(err);
        const errStr = String(err);
        if (
          parsed.code === "CF_VERIFICATION_REQUIRED" ||
          errStr.includes("403") ||
          errStr.toLowerCase().includes("forbidden") ||
          errStr.toLowerCase().includes("cloudflare")
        ) {
          console.warn(`[Browse] Categories fetch 403 Forbidden for ${site}, clearing invalid CF config.`);
          removeCfConfig(site);
          setError(t("browse_verify_error"));
        } else {
          showError(err);
        }
      } finally {
        setLoading(false);
      }
    };
    loadInitialData();
  }, [site, settings.language]);

  // 2. Fetch video list when state triggers
  useEffect(() => {
    if (!activeUrl && !searchKeyword) return;

    if (activeUrl) {
      if (site === Site.Jable && !activeUrl.includes("jable.tv")) return;
      if (site === Site.Missav && !activeUrl.includes("missav")) return;
      if (site === Site.Supjav && !activeUrl.includes("supjav")) return;
    }

    const loadVideos = async () => {
      setLoading(true);
      setError(null);
      try {
        let response: { videos: VideoItem[]; total_pages: number };
        const getLang = () => {
          if (site === Site.Jable) return settings.language === "en" ? "en" : settings.language === "ja" ? "jp" : "zh";
          if (site === Site.Missav) return settings.language === "zh-CN" ? "cn" : settings.language === "en" ? "en" : settings.language === "ja" ? "ja" : "";
          return settings.language === "en" ? "" : settings.language === "ja" ? "ja" : "zh";
        };
        const lang = getLang();

        if (searchKeyword) {
          response = await invoke("search_videos", {
            req: {
              site,
              keyword: searchKeyword,
              page,
              sortBy: sortBy || null,
              lang,
            },
          });
        } else {
          response = await invoke("fetch_video_list", {
            req: {
              site,
              url: activeUrl,
              page,
              sortBy: sortBy || null,
              lang,
            },
          });
        }

        setVideos(response.videos);
        setTotalPages(response.total_pages);
      } catch (err) {
        console.error("Failed to load videos:", err);
        const parsed = parseAppError(err);
        const errStr = String(err);
        if (
          parsed.code === "CF_VERIFICATION_REQUIRED" ||
          errStr.includes("403") ||
          errStr.toLowerCase().includes("forbidden") ||
          errStr.toLowerCase().includes("cloudflare") ||
          errStr.includes("遭")
        ) {
          console.warn(`[Browse] Videos fetch 403 Forbidden for ${site}, clearing invalid CF config.`);
          removeCfConfig(site);
          setError(t("browse_verify_error"));
        } else {
          showError(err);
          setError(typeof err === "string" ? err : t("browse_verify_error"));
        }
      } finally {
        setLoading(false);
      }
    };

    loadVideos();
  }, [activeUrl, searchKeyword, page, sortBy, site, settings.language]);

  const handleVerifyBypass = async () => {
    let targetUrl = "https://supjav.com/";
    if (site === Site.Missav) {
      targetUrl = "https://missav.ws/";
    } else if (site === Site.Jable) {
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
      if (
        (site === Site.Supjav && domain.includes("supjav")) ||
        (site === Site.Missav && domain.includes("missav")) ||
        (site === Site.Jable && domain.includes("jable"))
      ) {
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

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (!searchQuery.trim()) return;
    setActiveUrl("");
    setActiveTagUrl("");
    setSearchKeyword(searchQuery);
    setPage(1);
    setSortBy("");
  };

  const handleCategorySelect = (url: string) => {
    setSearchQuery("");
    setSearchKeyword(null);
    setActiveTagUrl("");
    setActiveUrl(url);
    setPage(1);
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

    // Skip videos that are already downloading/merging — starting a second
    // download for the same URL would corrupt the temp folder / output file.
    const targets = selectedVideos.filter((url) => {
      const existing = tasks[url];
      return (
        !existing ||
        !(existing.status === "downloading" || existing.status === "merging")
      );
    });
    const skipped = selectedVideos.length - targets.length;

    if (targets.length === 0) {
      showError("所选影片都已在下载中，无需重复添加");
      return;
    }

    let started = 0;
    const failures: string[] = [];
    for (const url of targets) {
      addTask(url, undefined, undefined, site);
      try {
        await invoke("start_download", {
          req: {
            site,
            url,
            saveDir,
            maxConcurrent,
            resolution,
          },
        });
        started += 1;
      } catch (err) {
        console.error(`Failed to start download for ${url}:`, err);
        failures.push(String(err));
      }
    }
    clearSelection();

    if (started > 0) {
      const skipText = skipped > 0 ? `（跳过 ${skipped} 个已在下载中）` : "";
      showSuccess(`已开始下载 ${started} 个影片${skipText}`);
      if (onNavigateToQueue) {
        onNavigateToQueue();
      }
    }
    if (failures.length > 0) {
      showError(
        `${failures.length} 个影片启动失败：${failures[0].replace(/^Error invoking remote function 'start_download': /, "")}`
      );
    }
  };

  const handleAddToQueueSelected = () => {
    if (selectedVideos.length === 0) return;
    selectedVideos.forEach((url) => {
      const match = videos.find((v) => v.url === url);
      const title = match ? (match.title as string) : "待解析...";
      addTask(url, title, "paused");
    });
    clearSelection();
    if (onNavigateToQueue) {
      onNavigateToQueue();
    }
  };

  return (
    <div className="flex-1 flex flex-col h-screen overflow-hidden bg-background text-foreground">
      {/* Top Header */}
      <header className="p-5 border-b border-border bg-card/40 backdrop-blur-md flex flex-col gap-4 select-none shrink-0 z-10">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-4">
          <div className="flex items-center justify-between sm:justify-start gap-4">
            <div className="flex items-center gap-2.5">
              <Compass className="w-5 h-5 text-primary shrink-0" />
              <h2 className="text-lg font-extrabold hidden sm:block">
                {site === Site.Jable ? "JableTV" : site === Site.Missav ? "MissAV" : "SupJav"}
              </h2>
            </div>

            {/* Site Selector buttons */}
            <div className="flex items-center bg-muted/60 p-1 rounded-xl border border-border">
              <Button
                variant={site === Site.Jable ? "default" : "ghost"}
                size="xs"
                onClick={() => setSite(Site.Jable)}
                className="font-bold px-3"
              >
                <span className="hidden sm:inline">JableTV</span>
                <span className="sm:hidden">J</span>
              </Button>
              <Button
                variant={site === Site.Missav ? "default" : "ghost"}
                size="xs"
                onClick={() => setSite(Site.Missav)}
                className="font-bold px-3"
              >
                <span className="hidden sm:inline">MissAV</span>
                <span className="sm:hidden">M</span>
              </Button>
              <Button
                variant={site === Site.Supjav ? "default" : "ghost"}
                size="xs"
                onClick={() => setSite(Site.Supjav)}
                className="font-bold px-3"
              >
                <span className="hidden sm:inline">SupJav</span>
                <span className="sm:hidden">S</span>
              </Button>
            </div>

            <Button
              variant="outline"
              size="xs"
              onClick={handleVerifyBypass}
              className={cn(
                "border-dashed font-bold flex items-center gap-1.5",
                isVerified ? "border-emerald-500 text-emerald-500 hover:bg-emerald-500/10" : "border-destructive text-destructive hover:bg-destructive/10"
              )}
              title={t("browse_verify_tooltip")}
            >
              <ShieldCheck className="w-3.5 h-3.5" />
              <span className="hidden sm:inline">
                {isVerified ? t("settings_cf_verified") : t("settings_cf_verify")}
              </span>
            </Button>
          </div>

          {/* Search form */}
          <form onSubmit={handleSearch} className="flex items-center gap-2 w-full lg:max-w-md">
            <div className="relative flex-1">
              <Input
                type="text"
                placeholder={t("browse_search_placeholder")}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-full font-medium pr-8"
              />
            </div>
            <Button type="submit" size="sm" className="px-4 font-bold shrink-0">
              <Search className="w-4 h-4 mr-1" />
              <span>{t("browse_search_btn")}</span>
            </Button>
          </form>
        </div>

        {/* Categories, Filters, Sorting & Batch Operations */}
        <div className="flex flex-row flex-nowrap items-center justify-between gap-2 w-full select-none pt-1">
          <div className="flex flex-row flex-nowrap items-center gap-2 shrink-0 overflow-x-auto">
            {/* Primary categories */}
            <div className="flex items-center bg-muted/50 p-1 rounded-lg border border-border/50">
              {categories.slice(0, 3).map((cat) => (
                <Button
                  key={cat.url}
                  variant={activeUrl === cat.url && !searchKeyword ? "default" : "ghost"}
                  size="xs"
                  onClick={() => handleCategorySelect(cat.url)}
                  className="font-bold text-xs px-2.5"
                >
                  {cat.name}
                </Button>
              ))}
            </div>

            {/* Other categories dropdown */}
            {categories.length > 3 && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="xs" className="font-bold">
                    {t("browse_more_categories")} ({categories.length - 3})
                    <ChevronDown className="w-3.5 h-3.5 ml-1" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="max-h-80 overflow-y-auto">
                  {categories.slice(3).map((cat) => (
                    <DropdownMenuItem
                      key={cat.url}
                      onClick={() => handleCategorySelect(cat.url)}
                      className={cn("font-medium text-xs cursor-pointer", activeUrl === cat.url && !searchKeyword && "font-bold text-primary")}
                    >
                      {cat.name}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            )}

            {/* Tag Filter Sheet Trigger (only for Jable) */}
            {site === Site.Jable && (
              <Sheet open={isSheetOpen} onOpenChange={setIsSheetOpen}>
                <SheetTrigger asChild>
                  <Button variant="outline" size="xs" className="font-bold border-primary/40 text-primary hover:bg-primary/10">
                    <Tag className="w-3.5 h-3.5 mr-1" />
                    <span>{t("browse_tag_filter")}</span>
                    {activeTagUrl && <Badge className="ml-1 text-[9px] px-1.5 py-0 font-bold bg-primary text-primary-foreground">{t("browse_tag_active").trim()}</Badge>}
                  </Button>
                </SheetTrigger>
                <SheetContent side="right" className="w-80 sm:max-w-md flex flex-col p-6">
                  <SheetHeader className="pb-4 border-b border-border mb-4">
                    <div className="flex items-center justify-between pr-6">
                      <SheetTitle className="text-primary flex items-center gap-2">
                        <Tag className="w-4 h-4" />
                        <span>{t("browse_tag_filter")}</span>
                      </SheetTitle>
                      {activeTagUrl && (
                        <Button
                          variant="outline"
                          size="xs"
                          onClick={() => {
                            if (categories.length > 0) {
                              handleCategorySelect(categories[0].url);
                            }
                            setIsSheetOpen(false);
                          }}
                          className="text-primary border-primary/40 hover:bg-primary/10 font-bold"
                        >
                          {t("browse_tag_clear")}
                        </Button>
                      )}
                    </div>
                  </SheetHeader>
                  <div className="flex-1 overflow-y-auto space-y-4 pr-1">
                    {Object.entries(sidebarTags).map(([group, tags]) => (
                      <div key={group} className="border border-border rounded-xl p-3 bg-muted/20">
                        <h4 className="text-xs font-bold text-primary mb-2">
                          {translateTagGroup(group, language)}
                        </h4>
                        <div className="flex flex-wrap gap-1.5">
                          {tags.map((tag) => (
                            <Button
                              key={tag.slug}
                              variant={activeTagUrl === tag.url ? "default" : "secondary"}
                              size="xs"
                              onClick={() => {
                                handleTagSelect(tag.url);
                                setIsSheetOpen(false);
                              }}
                              className="text-[11px] font-medium"
                            >
                              {translateTagName(tag.name, tag.slug, language)}
                            </Button>
                          ))}
                        </div>
                      </div>
                    ))}
                  </div>
                </SheetContent>
              </Sheet>
            )}

            {/* Sorting Selection dropdown (only for Jable) */}
            {site === Site.Jable && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="xs" className="font-bold">
                    <span>
                      {t("browse_sort_label")}{
                        sortBy === "post_date_and_popularity" ? t("browse_sort_recent_best") :
                          sortBy === "post_date" ? t("browse_sort_latest") :
                            sortBy === "video_viewed" ? t("browse_sort_most_view") :
                              sortBy === "most_favourited" ? t("browse_sort_most_fav") :
                                searchKeyword ? t("browse_sort_most_rel") : t("browse_sort_latest")
                      }
                    </span>
                    <ChevronDown className="w-3.5 h-3.5 ml-1" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start">
                  {searchKeyword && (
                    <DropdownMenuItem onClick={() => handleSortChange("")} className={cn("text-xs cursor-pointer", sortBy === "" && "font-bold text-primary")}>
                      {t("browse_sort_most_rel")}
                    </DropdownMenuItem>
                  )}
                  <DropdownMenuItem onClick={() => handleSortChange("post_date_and_popularity")} className={cn("text-xs cursor-pointer", sortBy === "post_date_and_popularity" && "font-bold text-primary")}>
                    {t("browse_sort_recent_best")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => handleSortChange("post_date")} className={cn("text-xs cursor-pointer", sortBy === "post_date" && "font-bold text-primary")}>
                    {t("browse_sort_latest")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => handleSortChange("video_viewed")} className={cn("text-xs cursor-pointer", sortBy === "video_viewed" && "font-bold text-primary")}>
                    {t("browse_sort_most_view")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => handleSortChange("most_favourited")} className={cn("text-xs cursor-pointer", sortBy === "most_favourited" && "font-bold text-primary")}>
                    {t("browse_sort_most_fav")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>

          {/* Batch action operations */}
          {videos.length > 0 && (
            <div className="flex flex-row flex-nowrap items-center gap-1.5 shrink-0">
              <Button
                variant="ghost"
                size="xs"
                onClick={handleToggleSelectAllCurrentPage}
                className="font-bold gap-1 text-xs"
              >
                {allCurrentPageSelected ? (
                  <CheckSquare className="w-3.5 h-3.5 text-primary" />
                ) : (
                  <Square className="w-3.5 h-3.5 text-muted-foreground" />
                )}
                <span className="hidden xs:inline">{t("browse_btn_select_all")}</span>
              </Button>
              <Button
                variant="outline"
                size="xs"
                onClick={handleAddToQueueSelected}
                disabled={selectedVideos.length === 0}
                className="font-bold gap-1 text-xs"
              >
                <Plus className="w-3.5 h-3.5" />
                <span>{t("browse_btn_queue")}</span>
                <span className="tabular-nums font-black">({selectedVideos.length})</span>
              </Button>
              <Button
                variant="default"
                size="xs"
                onClick={handleDownloadSelected}
                disabled={selectedVideos.length === 0}
                className="font-bold gap-1 text-xs"
              >
                <Download className="w-3.5 h-3.5" />
                <span>{t("browse_btn_download")}</span>
                <span className="tabular-nums font-black">({selectedVideos.length})</span>
              </Button>
            </div>
          )}
        </div>
      </header>

      {/* Video Grid & Pagination container */}
      <main className="flex-1 overflow-y-auto p-6 bg-background/50 relative">
        {error && (
          <div className="bg-destructive/15 border border-destructive/30 text-destructive font-bold p-4 rounded-xl shadow max-w-3xl mx-auto my-6 flex flex-col md:flex-row items-center justify-between gap-4">
            <span className="text-sm">{error}</span>
            <Button
              size="sm"
              onClick={handleVerifyBypass}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90 rounded-lg shadow font-extrabold gap-1.5 shrink-0"
            >
              <ShieldCheck className="w-4 h-4 animate-bounce" />
              {t("browse_verify_trigger")}
            </Button>
          </div>
        )}

        {loading && videos.length === 0 ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-5">
            {Array.from({ length: 12 }).map((_, idx) => (
              <div key={idx} className="flex flex-col gap-3 w-full animate-pulse">
                <div className="aspect-video w-full rounded-xl bg-muted"></div>
                <div className="h-4 w-28 rounded bg-muted"></div>
                <div className="h-4 w-full rounded bg-muted"></div>
              </div>
            ))}
          </div>
        ) : !loading && videos.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full py-16 text-center select-none animate-fade-in">
            <Loader2 className="w-16 h-16 text-muted-foreground/30 animate-spin mb-4" />
            <h3 className="font-extrabold text-lg text-muted-foreground">{t("browse_empty")}</h3>
            <p className="text-sm text-muted-foreground/70 mt-1">{t("browse_empty_desc")}</p>
          </div>
        ) : (
          <div className={cn("flex flex-col h-full justify-between transition-opacity duration-300 relative", loading && "opacity-40 pointer-events-none")}>
            {loading && (
              <div className="absolute top-4 left-1/2 -translate-x-1/2 z-30 bg-primary text-primary-foreground px-4 py-1.5 rounded-full text-xs font-extrabold shadow-lg flex items-center gap-2 animate-bounce">
                <Loader2 className="w-4 h-4 animate-spin" />
                <span>{t("browse_loading")}</span>
              </div>
            )}
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
              <div className="flex items-center justify-center mt-12 py-6 select-none border-t border-border/50">
                <div className="flex flex-col sm:flex-row items-center gap-4 bg-card/60 p-2 rounded-2xl border border-border shadow-sm">
                  <div className="flex items-center gap-1">
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => handlePageChange(1)}
                      disabled={page === 1}
                      title={t("browse_btn_first")}
                    >
                      <ChevronsLeft className="w-4 h-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => handlePageChange(page - 1)}
                      disabled={page === 1}
                      title={t("browse_btn_prev")}
                    >
                      <ChevronLeft className="w-4 h-4" />
                    </Button>

                    <span className="text-xs font-bold tracking-wider text-muted-foreground px-4 min-w-[120px] text-center">
                      {t("browse_page_indicator", { page, total: totalPages })}
                    </span>

                    <Button
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => handlePageChange(page + 1)}
                      disabled={page === totalPages}
                      title={t("browse_btn_next")}
                    >
                      <ChevronRight className="w-4 h-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => handlePageChange(totalPages)}
                      disabled={page === totalPages}
                      title={t("browse_btn_last")}
                    >
                      <ChevronsRight className="w-4 h-4" />
                    </Button>
                  </div>

                  <div className="hidden sm:block w-[1px] h-6 bg-border mx-1"></div>

                  <form onSubmit={handleJumpPageSubmit} className="flex items-center gap-2 pl-1 pr-2">
                    <Input
                      type="text"
                      value={jumpPageInput}
                      onChange={handleJumpInputChange}
                      className="w-12 h-8 text-center font-bold p-0 text-xs"
                    />
                    <Button
                      type="submit"
                      size="xs"
                      disabled={!jumpPageInput}
                      className="h-8 font-bold px-3"
                    >
                      {t("browse_page_jump")}
                    </Button>
                  </form>
                </div>
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  );
};
