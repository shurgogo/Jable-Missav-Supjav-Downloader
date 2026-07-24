import React, { useState, useRef, useEffect } from "react";
import { Check } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import { useDownloadStore } from "../store/useDownloadStore";
import { Badge } from "./ui/badge";
import { cn } from "../lib/utils";

export interface VideoItem {
  title: String;
  url: String;
  image_url: String;
  duration: String | null;
  preview_url?: String | null;
}

interface VideoCardProps {
  video: VideoItem;
  isSelected: boolean;
  onToggle: () => void;
  onDoubleClick: () => void;
}

// In-memory cache mapping previewUrl -> Blob objectUrl
const previewBlobCache = new Map<string, string>();
const fetchingPreviews = new Set<string>();

export const VideoCard: React.FC<VideoCardProps> = ({
  video,
  isSelected,
  onToggle,
  onDoubleClick,
}) => {
  const { t } = useTranslation();
  const activeSite = useDownloadStore((state) => state.activeSite);
  const [isHovered, setIsHovered] = useState(false);
  const [videoSrc, setVideoSrc] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  const previewUrlStr = video.preview_url as string | undefined;

  const handleMouseEnter = () => {
    setIsHovered(true);
    if (!previewUrlStr) return;

    if (previewBlobCache.has(previewUrlStr)) {
      setVideoSrc(previewBlobCache.get(previewUrlStr)!);
    } else {
      // Set to direct previewUrl initially for zero-delay streaming
      setVideoSrc(previewUrlStr);

      // Fetch blob into memory cache asynchronously via Tauri backend
      if (!fetchingPreviews.has(previewUrlStr)) {
        fetchingPreviews.add(previewUrlStr);
        invoke<number[]>("fetch_preview_video", { req: { site: activeSite, url: previewUrlStr } })
          .then((bytes) => {
            const blob = new Blob([new Uint8Array(bytes)], { type: "video/mp4" });
            const objectUrl = URL.createObjectURL(blob);
            previewBlobCache.set(previewUrlStr, objectUrl);
            fetchingPreviews.delete(previewUrlStr);
            setVideoSrc(objectUrl);
          })
          .catch((err) => {
            fetchingPreviews.delete(previewUrlStr);
            console.warn("Could not cache preview blob in memory:", err);
          });
      }
    }
  };

  const handleMouseLeave = () => {
    setIsHovered(false);
    if (videoRef.current) {
      videoRef.current.pause();
    }
  };

  useEffect(() => {
    if (isHovered && videoRef.current && videoSrc) {
      const playPromise = videoRef.current.play();
      if (playPromise !== undefined) {
        playPromise.catch(() => {
          // Auto-play was prevented or interrupted
        });
      }
    }
  }, [isHovered, videoSrc]);

  return (
    <div
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onDoubleClick={onDoubleClick}
      onClick={onToggle}
      className={cn(
        "bg-card text-card-foreground rounded-xl border shadow-sm overflow-hidden cursor-pointer transition-all duration-300 transform hover:-translate-y-1 hover:shadow-lg group flex flex-col justify-between",
        isSelected
          ? "border-primary ring-2 ring-primary/30 bg-primary/5"
          : "border-border hover:border-primary/50"
      )}
    >
      {/* Thumbnail Frame */}
      <div className="relative aspect-video bg-black overflow-hidden select-none shrink-0">
        <img
          src={video.image_url as string}
          alt={video.title as string}
          loading="lazy"
          referrerPolicy="no-referrer"
          className="w-full h-full object-cover transition-transform duration-500 group-hover:scale-105"
        />

        {/* Hover Video Preview Player */}
        {isHovered && previewUrlStr && videoSrc && (
          <video
            ref={videoRef}
            src={videoSrc}
            autoPlay
            loop
            muted
            playsInline
            className="absolute inset-0 w-full h-full object-cover z-10 animate-fade-in"
          />
        )}

        {/* Preview Playing Badge */}
        {isHovered && previewUrlStr && (
          <Badge className="absolute top-2 left-2 z-20 bg-primary/90 text-white text-[10px] font-black tracking-wider uppercase shadow flex items-center gap-1.5 animate-pulse border-none">
            <span className="w-1.5 h-1.5 rounded-full bg-white animate-ping"></span>
            PREVIEW
          </Badge>
        )}

        {/* Selected Overlay Checkmark */}
        {isSelected && (
          <div className="absolute inset-0 bg-primary/25 backdrop-blur-[1px] flex items-center justify-center transition-all duration-300 z-20">
            <div className="w-12 h-12 rounded-full bg-primary text-white flex items-center justify-center shadow-lg transform scale-110 animate-fade-in">
              <Check className="w-6 h-6 stroke-[3]" />
            </div>
          </div>
        )}

        {/* Duration Label */}
        {video.duration && (
          <span className="absolute bottom-2 right-2 z-20 px-2 py-0.5 rounded-md bg-black/80 backdrop-blur text-white text-[11px] font-extrabold tracking-wider font-mono">
            {video.duration}
          </span>
        )}
      </div>

      {/* Video Content */}
      <div className="p-4 flex flex-col justify-between flex-1 min-h-[96px]">
        <h3
          className={cn(
            "font-semibold text-sm line-clamp-2 transition-colors duration-200 leading-snug",
            isSelected ? "text-primary font-bold" : "text-foreground group-hover:text-primary"
          )}
          title={video.title as string}
        >
          {video.title}
        </h3>

        {/* Source indicator */}
        <div className="flex items-center justify-between text-[11px] text-muted-foreground font-bold uppercase tracking-wider select-none mt-3 pt-2 border-t border-border/50">
          <span>
            {video.url.includes("missav")
              ? "MissAV"
              : video.url.includes("supjav")
              ? "SupJav"
              : "JableTV"}
          </span>
          <span className="hover:text-primary transition-colors">
            {t("browse_double_click_watch")}
          </span>
        </div>
      </div>
    </div>
  );
};
