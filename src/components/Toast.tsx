import React, { useEffect } from "react";
import { AlertTriangle, CheckCircle, Info, XCircle, X } from "lucide-react";
import { useToastStore, ToastItem } from "../store/useToastStore";
import { useTranslation } from "../i18n";
import { formatErrorMessage } from "../utils/error";

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

  const alertClass =
    item.type === "error"
      ? "alert-error text-white"
      : item.type === "success"
      ? "alert-success text-white"
      : item.type === "warning"
      ? "alert-warning text-white"
      : "alert-info text-white";

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
      className={`alert ${alertClass} shadow-lg rounded-2xl flex items-center justify-between gap-3 p-3.5 text-xs sm:text-sm font-bold animate-in fade-in slide-in-from-top-4 duration-300 max-w-md w-full border border-white/10`}
    >
      <div className="flex items-center gap-2.5 min-w-0 flex-1">
        {renderIcon()}
        <span className="truncate break-words">{displayMessage}</span>
      </div>
      <button
        onClick={() => removeToast(item.id)}
        className="btn btn-square btn-ghost btn-xs text-white/70 hover:text-white shrink-0"
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
};

export const ToastContainer: React.FC = () => {
  const toasts = useToastStore((state) => state.toasts);

  if (toasts.length === 0) return null;

  return (
    <div className="toast toast-top toast-end z-[9999] p-4 space-y-2 pointer-events-auto">
      {toasts.map((item) => (
        <ToastSingle key={item.id} item={item} />
      ))}
    </div>
  );
};
