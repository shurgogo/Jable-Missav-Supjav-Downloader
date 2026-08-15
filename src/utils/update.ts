import { invoke } from "@tauri-apps/api/core";
import { useUpdateStore, UpdateInfo } from "../store/useUpdateStore";

export interface UpdateCheckResult {
  info: UpdateInfo | null;
  failed: boolean;
}

export interface UpdateCheckOptions {
  /**
   * When true (default), a version the user explicitly skipped is hidden.
   * Manual checks from the Settings page pass false so the user can always
   * see and reach the newest release again.
   */
  respectIgnored?: boolean;
}

/**
 * Ask the backend for the latest release and compare it with the running
 * version. When an update is available (and not explicitly skipped), it is
 * stored in the update store (which lights up the sidebar badge). The dialog
 * is NOT opened here — callers decide that.
 */
export async function runUpdateCheck(
  options: UpdateCheckOptions = {}
): Promise<UpdateCheckResult> {
  const respectIgnored = options.respectIgnored !== false;
  const { setUpdateInfo, setChecking, ignoredVersion } = useUpdateStore.getState();
  setChecking(true);
  try {
    const info = await invoke<UpdateInfo>("check_for_update");
    if (info.updateAvailable && (!respectIgnored || info.latestVersion !== ignoredVersion)) {
      setUpdateInfo(info);
      return { info, failed: false };
    }
    setUpdateInfo(null);
    return { info: null, failed: false };
  } catch (err) {
    // Offline / rate-limited / API moved: never nag.
    console.warn("Update check failed:", err);
    return { info: null, failed: true };
  } finally {
    setChecking(false);
  }
}
