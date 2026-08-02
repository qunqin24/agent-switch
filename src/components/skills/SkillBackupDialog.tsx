import { History, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { SkillBackupEntry } from "@/lib/api/skills";

function formatSkillBackupDate(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  return Number.isNaN(date.getTime())
    ? String(unixSeconds)
    : date.toLocaleString();
}

export function SkillBackupDialog({
  backups,
  isDeleting,
  isLoading,
  isRestoring,
  onDelete,
  onRestore,
  onClose,
  open,
}: {
  backups: SkillBackupEntry[];
  isDeleting: boolean;
  isLoading: boolean;
  isRestoring: boolean;
  onDelete: (backup: SkillBackupEntry) => void;
  onRestore: (backupId: string) => void;
  onClose: () => void;
  open: boolean;
}) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onClose()}>
      <DialogContent className="max-h-[70vh] max-w-2xl" zIndex="nested">
        <DialogHeader>
          <DialogTitle>{t("skills.restoreFromBackup.title")}</DialogTitle>
          <DialogDescription>
            {t("skills.restoreFromBackup.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-[48vh] space-y-2 overflow-y-auto">
          {isLoading ? (
            <div className="flex justify-center py-10">
              <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
            </div>
          ) : backups.length === 0 ? (
            <div className="py-10 text-center text-sm text-muted-foreground">
              <History className="mx-auto mb-3 h-8 w-8 opacity-40" />
              {t("skills.restoreFromBackup.empty")}
            </div>
          ) : (
            backups.map((backup) => (
              <div
                key={backup.backupId}
                className="flex items-center gap-3 rounded-lg border border-border p-3"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {backup.skill.name}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t("skills.restoreFromBackup.createdAt")}:{" "}
                    {formatSkillBackupDate(backup.createdAt)}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={isRestoring || isDeleting}
                  onClick={() => onRestore(backup.backupId)}
                >
                  {isRestoring
                    ? t("skills.restoreFromBackup.restoring")
                    : t("skills.restoreFromBackup.restore")}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={isRestoring || isDeleting}
                  onClick={() => onDelete(backup)}
                >
                  {isDeleting
                    ? t("skills.restoreFromBackup.deleting")
                    : t("skills.restoreFromBackup.delete")}
                </Button>
              </div>
            ))
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
