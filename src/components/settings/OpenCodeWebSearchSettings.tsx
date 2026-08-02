import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ToggleRow } from "@/components/ui/toggle-row";
import { configApi } from "@/lib/api";

export function OpenCodeWebSearchSettings() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let active = true;

    void configApi
      .getOpenCodeWebSearchEnabled()
      .then((value) => {
        if (active) setEnabled(value);
      })
      .catch((error) => {
        console.error("Failed to read OpenCode web search setting", error);
        if (active) {
          toast.error(t("settings.openCodeWebSearch.loadFailed"));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [t]);

  const updateEnabled = useCallback(
    async (nextEnabled: boolean) => {
      setSaving(true);
      try {
        await configApi.setOpenCodeWebSearchEnabled(nextEnabled);
        setEnabled(nextEnabled);
        toast.success(
          t(
            nextEnabled
              ? "settings.openCodeWebSearch.enabled"
              : "settings.openCodeWebSearch.disabled",
          ),
        );
      } catch (error) {
        console.error("Failed to update OpenCode web search setting", error);
        toast.error(t("settings.openCodeWebSearch.saveFailed"));
      } finally {
        setSaving(false);
      }
    },
    [t],
  );

  return (
    <ToggleRow
      variant="plain"
      title={t("settings.openCodeWebSearch.title")}
      description={t("settings.openCodeWebSearch.description")}
      checked={enabled}
      disabled={loading || saving}
      onCheckedChange={(value) => void updateEnabled(value)}
    />
  );
}
