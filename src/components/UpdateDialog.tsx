import React from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, Rocket } from "lucide-react";
import { useTranslation } from "../i18n";
import { useUpdateStore } from "../store/useUpdateStore";
import { useToastStore } from "../store/useToastStore";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

/**
 * Very light Markdown-ish rendering for GitHub release notes: `#` headings
 * become coloured group titles, `-`/`*` items become bullet lists. Good
 * enough for a changelog without pulling in a Markdown dependency.
 */
function renderChangelog(body: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  body.split("\n").forEach((line, i) => {
    const trimmed = line.trim();
    if (/^#{1,4}\s/.test(trimmed)) {
      nodes.push(
        <p
          key={i}
          className="mt-3 mb-1.5 text-sm font-bold text-primary first:mt-0"
        >
          {trimmed.replace(/^#+\s*/, "")}
        </p>
      );
    } else if (/^[-*]\s/.test(trimmed)) {
      nodes.push(
        <p key={i} className="pl-4 text-[13px] leading-relaxed text-foreground/85">
          • {trimmed.replace(/^[-*]\s*/, "")}
        </p>
      );
    } else if (trimmed === "") {
      nodes.push(<div key={i} className="h-1.5" />);
    } else {
      nodes.push(
        <p key={i} className="text-[13px] leading-relaxed text-foreground/85">
          {line}
        </p>
      );
    }
  });
  return nodes;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString();
}

export const UpdateDialog: React.FC = () => {
  const { t } = useTranslation();
  const dialogOpen = useUpdateStore((s) => s.dialogOpen);
  const setDialogOpen = useUpdateStore((s) => s.setDialogOpen);
  const updateInfo = useUpdateStore((s) => s.updateInfo);
  const skipVersion = useUpdateStore((s) => s.skipVersion);
  const showError = useToastStore((s) => s.showError);
  const showInfo = useToastStore((s) => s.showInfo);

  if (!updateInfo) return null;

  const handleUpdate = async () => {
    try {
      await openUrl(updateInfo.releaseUrl);
    } catch (err) {
      console.error("Failed to open release page:", err);
      showError(`无法打开更新页面: ${err}`);
    }
    setDialogOpen(false);
  };

  const handleSkip = () => {
    skipVersion(updateInfo.latestVersion);
    // Tell the user what happened and that it is reversible.
    showInfo(t("update_skip_notice", { version: updateInfo.latestVersion }));
  };

  return (
    <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
      <DialogContent className="max-w-md">
        {/* Header: icon + title + subtitle (no visual banner) */}
        <DialogHeader className="flex-row items-center gap-3 space-y-0 text-left">
          <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
            <Rocket className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <DialogTitle className="text-lg font-bold leading-tight truncate">
              {t("update_title")}
            </DialogTitle>
            <DialogDescription className="text-xs mt-0.5 truncate">
              {t("update_ready", { version: updateInfo.latestVersion })}
              {updateInfo.publishedAt
                ? ` · ${t("update_published", {
                    date: formatDate(updateInfo.publishedAt),
                  })}`
                : ""}
            </DialogDescription>
          </div>
        </DialogHeader>

        {/* Change log */}
        <div className="space-y-1.5">
          <p className="text-[11px] font-bold uppercase tracking-wider text-muted-foreground select-none">
            {t("update_changelog")}
          </p>
          <div className="max-h-48 overflow-y-auto rounded-xl border border-border bg-muted/40 p-4 text-left">
            {updateInfo.changelog.trim() ? (
              renderChangelog(updateInfo.changelog)
            ) : (
              <p className="text-xs text-muted-foreground">
                {t("update_no_notes")}
              </p>
            )}
          </div>
        </div>

        {/* Actions: skip on the left, primary actions on the right */}
        <div className="flex items-center justify-between gap-2">
          <button
            type="button"
            onClick={handleSkip}
            className="px-3 py-2 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
            title={t("update_skip_hint")}
          >
            {t("update_skip")}
          </button>

          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setDialogOpen(false)}
            >
              {t("update_later")}
            </Button>
            <Button size="sm" onClick={handleUpdate} className="gap-1.5 font-bold">
              <Download className="w-3.5 h-3.5" />
              {t("update_now")}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
};
