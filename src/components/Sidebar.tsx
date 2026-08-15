import React, { useState, useEffect } from "react";
import { Compass, DownloadCloud, Settings, BarChart3 } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { useTranslation } from "../i18n";
import { useUpdateStore } from "../store/useUpdateStore";
import { Badge } from "./ui/badge";
import { cn } from "../lib/utils";

interface SidebarProps {
  activeTab: string;
  setActiveTab: (tab: string) => void;
  selectedCount: number;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeTab,
  setActiveTab,
  selectedCount,
}) => {
  const { t } = useTranslation();
  const [isCollapsed, setIsCollapsed] = useState<boolean>(window.innerWidth < 768);
  const [appVersion, setAppVersion] = useState<string>("0.1.2");
  const updateInfo = useUpdateStore((s) => s.updateInfo);
  const setDialogOpen = useUpdateStore((s) => s.setDialogOpen);

  useEffect(() => {
    getVersion().then((v) => setAppVersion(v)).catch(() => {});
  }, []);

  useEffect(() => {
    const handleResize = () => {
      if (window.innerWidth < 768) {
        setIsCollapsed(true);
      } else {
        setIsCollapsed(false);
      }
    };
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  const menuItems = [
    { id: "browse", name: t("nav_browse"), icon: Compass },
    { id: "queue", name: t("nav_queue"), icon: DownloadCloud, badge: selectedCount > 0 ? selectedCount : undefined },
    { id: "stats", name: t("nav_stats"), icon: BarChart3 },
    { id: "settings", name: t("nav_settings"), icon: Settings },
  ];

  return (
    <aside
      className={cn(
        "bg-card/90 backdrop-blur-md border-r border-border flex flex-col h-screen select-none transition-all duration-300 shrink-0 z-20",
        isCollapsed ? "w-20" : "w-64"
      )}
    >
      {/* App Logo/Header */}
      <div
        onClick={() => setIsCollapsed(!isCollapsed)}
        className={cn(
          "relative p-5 flex items-center border-b border-border bg-muted/30 cursor-pointer hover:bg-muted/60 transition-colors duration-200",
          isCollapsed ? "justify-center" : "gap-3"
        )}
        title={isCollapsed ? t("sidebar_expand") : t("sidebar_collapse")}
      >
        <div className="w-10 h-10 shrink-0 flex items-center justify-center">
          <img src="/app-icon.png" alt="AVDL Logo" className="w-full h-full object-contain drop-shadow-sm" />
        </div>
        {/* Update dot when collapsed (no room for the version text) */}
        {isCollapsed && updateInfo && (
          <span
            className="absolute top-3 right-3 w-2.5 h-2.5 rounded-full bg-primary animate-pulse ring-2 ring-background"
            title={`${t("update_title")} ${updateInfo.latestVersion}`}
          />
        )}
        {!isCollapsed && (
          <div className="animate-fade-in overflow-hidden min-w-0">
            <h1 className="font-extrabold text-sm leading-tight text-foreground truncate">
              AVDL
            </h1>
            <button
              onClick={(e) => {
                e.stopPropagation();
                if (updateInfo) {
                  setDialogOpen(true);
                }
              }}
              className={cn(
                "inline-flex items-center gap-1.5 text-[10px] font-bold tracking-wider uppercase leading-tight",
                updateInfo
                  ? "text-primary hover:text-primary/80 cursor-pointer"
                  : "text-muted-foreground cursor-default"
              )}
              title={updateInfo ? `${t("update_title")} ${updateInfo.latestVersion}` : t("update_up_to_date")}
            >
              <span>v{appVersion}</span>
              {updateInfo && (
                <Badge
                  variant="default"
                  className="px-1.5 py-px text-[8px] font-black leading-none animate-pulse bg-primary text-primary-foreground"
                >
                  NEW
                </Badge>
              )}
            </button>
          </div>
        )}
      </div>

      {/* Navigation Menu */}
      <nav className="flex-1 px-3 py-6 space-y-1.5 overflow-y-auto">
        {menuItems.map((item) => {
          const Icon = item.icon;
          const isActive = activeTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={cn(
                "w-full flex items-center py-3 rounded-xl font-bold transition-all duration-200 group relative cursor-pointer",
                isCollapsed ? "justify-center px-0" : "px-4 gap-3.5",
                isActive
                  ? "bg-primary text-primary-foreground shadow-md shadow-primary/20"
                  : "text-muted-foreground hover:bg-accent hover:text-foreground"
              )}
              title={isCollapsed ? item.name : undefined}
            >
              <Icon
                className={cn(
                  "w-5 h-5 transition-transform duration-200 group-hover:scale-110",
                  isActive ? "text-primary-foreground" : "text-muted-foreground group-hover:text-foreground"
                )}
              />
              {!isCollapsed && <span className="flex-1 text-left animate-fade-in text-sm">{item.name}</span>}
              {item.badge !== undefined && (
                <Badge
                  variant="destructive"
                  className={cn(
                    "font-bold text-white shadow-sm",
                    isCollapsed
                      ? "absolute top-1 right-2 scale-75 animate-bounce px-1.5 min-h-0 h-4 min-w-0 w-4 rounded-full flex items-center justify-center text-[9px] p-0"
                      : "px-2 py-0.5 text-xs animate-bounce"
                  )}
                >
                  {item.badge}
                </Badge>
              )}
            </button>
          );
        })}
      </nav>

      {/* Footer Info */}
      {!isCollapsed && (
        <div className="p-5 border-t border-border text-center text-xs text-muted-foreground font-semibold bg-muted/20 animate-fade-in">
          <a
            href="https://v2.tauri.app/"
            target="_blank"
            rel="noopener noreferrer"
            className="hover:text-primary transition-colors duration-200"
          >
            Powered by Tauri
          </a>
        </div>
      )}
    </aside>
  );
};
