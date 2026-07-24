import React, { useEffect } from "react";
import { AlertTriangle, CheckCircle, Info, XCircle, X } from "lucide-react";
import { useToastStore, ToastItem } from "../store/useToastStore";
import { useTranslation } from "../i18n";
import { formatErrorMessage } from "../utils/error";
import { Button } from "./ui/button";
import { cn } from "../lib/utils";

const ToastSingle: React.FC<{ item: ToastItem }> = ({ item }) => {
  const { t } = useTranslation();
  const removeToast = useToastStore((state) => state.removeToast);

  useEffect(() => {
    const timer = setTimeout(() => {
      removeToast(item.id);
    }, item.duration || 4000);
    return () => clearTimeout(timer);
  }, [item.id, item.duration, removeToast]);

  const displayMessage =
    item.type === "error" && item.error
      ? formatErrorMessage(item.error, t)
      : item.message || "";

  const alertVariantClass =
    item.type === "error"
      ? "bg-destructive text-destructive-foreground border-destructive/20"
      : item.type === "success"
      ? "bg-emerald-600 text-white border-emerald-500/20"
      : item.type === "warning"
      ? "bg-amber-600 text-white border-amber-500/20"
      : "bg-primary text-primary-foreground border-primary/20";

  const renderIcon = () => {
    switch (item.type) {
      case "error":
        return <XCircle className="w-5 h-5 shrink-0" />;
      case "success":
        return <CheckCircle className="w-5 h-5 shrink-0" />;
      case "warning":
        return <AlertTriangle className="w-5 h-5 shrink-0" />;
      default:
        return <Info className="w-5 h-5 shrink-0" />;
    }
  };

  return (
    <div
      className={cn(
        "shadow-lg rounded-xl flex items-center justify-between gap-3 p-3.5 text-xs sm:text-sm font-semibold animate-in fade-in slide-in-from-top-4 duration-200 max-w-md w-full border backdrop-blur-md",
        alertVariantClass
      )}
    >
      <div className="flex items-center gap-2.5 min-w-0 flex-1">
        {renderIcon()}
        <span className="truncate break-words">{displayMessage}</span>
      </div>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={() => removeToast(item.id)}
        className="text-white/80 hover:text-white hover:bg-white/10 shrink-0"
      >
        <X className="w-4 h-4" />
      </Button>
    </div>
  );
};

export const ToastContainer: React.FC = () => {
  const toasts = useToastStore((state) => state.toasts);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed top-4 right-4 z-[9999] p-4 space-y-2 pointer-events-auto flex flex-col items-end">
      {toasts.map((item) => (
        <ToastSingle key={item.id} item={item} />
      ))}
    </div>
  );
};
