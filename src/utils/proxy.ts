import { invoke } from "@tauri-apps/api/core";
import { useDownloadStore } from "../store/useDownloadStore";

export interface ProxyStatus {
  mode: "system" | "direct" | "custom";
  /** Effective proxy URL, or null when direct. */
  url: string | null;
  /** Warning code ("pac_unsupported" | "parse_failed" | "socks_unsupported"), or null. */
  warning: string | null;
}

/** Apply the persisted proxy settings to the backend HTTP client. */
export async function applyProxySettings(): Promise<ProxyStatus> {
  const { settings } = useDownloadStore.getState();
  return invoke<ProxyStatus>("apply_proxy_settings", {
    settings: {
      mode: settings.proxyMode || "system",
      customProxy: settings.customProxy || "",
    },
  });
}

/** Read the currently effective proxy status from the backend. */
export function getProxyStatus(): Promise<ProxyStatus> {
  return invoke<ProxyStatus>("get_proxy_status");
}
