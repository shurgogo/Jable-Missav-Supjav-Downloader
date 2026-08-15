import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  changelog: string;
  releaseUrl: string;
  publishedAt?: string | null;
}

interface UpdateState {
  updateInfo: UpdateInfo | null;
  dialogOpen: boolean;
  checking: boolean;
  /** Version the user chose to skip ("ignore this version"). */
  ignoredVersion: string | null;
  setUpdateInfo: (info: UpdateInfo | null) => void;
  setDialogOpen: (open: boolean) => void;
  setChecking: (checking: boolean) => void;
  skipVersion: (version: string) => void;
  clearIgnoredVersion: () => void;
}

export const useUpdateStore = create<UpdateState>()(
  persist(
    (set) => ({
      updateInfo: null,
      dialogOpen: false,
      checking: false,
      ignoredVersion: null,
      setUpdateInfo: (info) => set({ updateInfo: info }),
      setDialogOpen: (open) => set({ dialogOpen: open }),
      setChecking: (checking) => set({ checking }),
      skipVersion: (version) =>
        set({
          ignoredVersion: version,
          dialogOpen: false,
          updateInfo: null,
        }),
      clearIgnoredVersion: () => set({ ignoredVersion: null }),
    }),
    {
      name: "avdl-update-store",
      // Only the user's "ignore this version" choice survives restarts.
      partialize: (state) => ({ ignoredVersion: state.ignoredVersion }),
    }
  )
);
