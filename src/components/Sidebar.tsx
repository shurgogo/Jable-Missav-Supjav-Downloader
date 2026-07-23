import React, { useState, useEffect } from "react";
import { Compass, DownloadCloud, Settings, BarChart3 } from "lucide-react";
import { useTranslation } from "../i18n";

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
    <aside className={`bg-base-300 border-r border-base-100 flex flex-col h-screen select-none transition-all duration-300 shrink-0 ${isCollapsed ? "w-20" : "w-64"}`}>
      {/* App Logo/Header */}
      <div
        onClick={() => setIsCollapsed(!isCollapsed)}
        className={`p-5 flex items-center border-b border-base-100 bg-base-300/50 backdrop-blur cursor-pointer hover:bg-base-200/50 transition-colors duration-200 ${isCollapsed ? "justify-center" : "gap-3"}`}
        title={isCollapsed ? t("sidebar_expand") : t("sidebar_collapse")}
      >
        <div className="w-10 h-10 animate-pulse shrink-0 flex items-center justify-center">
          <img src="/app-icon.png" alt="AVDL Logo" className="w-full h-full object-contain" />
        </div>
        {!isCollapsed && (
          <div className="animate-fade-in overflow-hidden">
            <h1 className="font-extrabold text-sm leading-tight bg-clip-text text-transparent bg-gradient-to-r from-base-content to-base-content/80 truncate">
              AVDL
            </h1>
            <span className="text-[10px] text-error font-semibold tracking-wider uppercase block">
              v0.1.1
            </span>
          </div>
        )}
      </div>

      {/* Navigation Menu */}
      <nav className="flex-1 px-4 py-6 space-y-2 overflow-y-auto">
        {menuItems.map((item) => {
          const Icon = item.icon;
          const isActive = activeTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={`w-full flex items-center py-3.5 rounded-xl font-bold transition-all duration-300 transform group hover:scale-[1.02] relative ${isCollapsed ? "justify-center px-0" : "px-4 gap-4"
                } ${isActive
                  ? "bg-gradient-to-r from-error to-pink-600 text-white shadow-lg shadow-error/30"
                  : "text-base-content/70 hover:bg-base-200 hover:text-base-content"
                }`}
              title={isCollapsed ? item.name : undefined}
            >
              <Icon
                className={`w-5 h-5 transition-transform duration-300 group-hover:rotate-6 ${isActive ? "text-white" : "text-base-content/50 group-hover:text-base-content"
                  }`}
              />
              {!isCollapsed && <span className="flex-1 text-left animate-fade-in">{item.name}</span>}
              {item.badge !== undefined && (
                <span className={`badge badge-error border-none font-bold text-white ${isCollapsed
                  ? "absolute top-1 right-2 scale-75 animate-bounce px-1.5 min-h-0 h-4 min-w-0 w-4 rounded-full flex items-center justify-center text-[9px]"
                  : "px-2 py-0.5 text-xs animate-bounce"
                  }`}>
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </nav>

      {/* Footer Info */}
      {!isCollapsed && (
        <div className="p-6 border-t border-base-100 text-center text-xs text-base-content/40 font-semibold bg-base-300/30 animate-fade-in">
          <a href="https://v2.tauri.app/" target="_blank" rel="noopener noreferrer" className="hover:text-blue-500 hover:underline transition-colors duration-300">Powered by Tauri</a>
        </div>
      )}
    </aside>
  );
};
