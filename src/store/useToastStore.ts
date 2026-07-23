import { create } from "zustand";
import { AppError, parseAppError } from "../utils/error";

export interface ToastItem {
  id: string;
  type: "error" | "success" | "info" | "warning";
  error?: AppError;
  message?: string;
  duration?: number;
}

interface ToastState {
  toasts: ToastItem[];
  showError: (err: unknown) => void;
  showSuccess: (message: string) => void;
  showInfo: (message: string) => void;
  removeToast: (id: string) => void;
}

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  showError: (err) => {
    const parsed = parseAppError(err);
    const id = Math.random().toString(36).substring(2, 9);
    set((state) => ({
      toasts: [...state.toasts, { id, type: "error", error: parsed, duration: 5000 }],
    }));
  },
  showSuccess: (message) => {
    const id = Math.random().toString(36).substring(2, 9);
    set((state) => ({
      toasts: [...state.toasts, { id, type: "success", message, duration: 3000 }],
    }));
  },
  showInfo: (message) => {
    const id = Math.random().toString(36).substring(2, 9);
    set((state) => ({
      toasts: [...state.toasts, { id, type: "info", message, duration: 4000 }],
    }));
  },
  removeToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),
}));
