import { useCallback, useEffect, useMemo, useState } from "react";
import { Check, ChevronsUpDown, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { SettingRow } from "@/components/settings/SettingRow";
import { providersApi, configApi } from "@/lib/api";
import type { OmoOpenCodeModel } from "@/lib/api/providers";
import {
  readCachedOmoModelCatalog,
  writeCachedOmoModelCatalog,
} from "@/components/providers/forms/hooks/useOmoModelSource";
import { cn } from "@/lib/utils";

export function OpenCodeSmallModelSettings() {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const [models, setModels] = useState<OmoOpenCodeModel[]>(
    () => readCachedOmoModelCatalog() ?? [],
  );
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let active = true;

    void configApi
      .getOpenCodeSmallModel()
      .then((model) => {
        if (active) setValue(model ?? "");
      })
      .catch((error) => {
        console.error("Failed to read OpenCode small_model", error);
        if (active) {
          toast.error(t("settings.openCodeSmallModel.loadFailed"));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    void providersApi
      .listOpenCodeModelsForOmo()
      .then((catalog) => {
        writeCachedOmoModelCatalog(catalog);
        if (active) setModels(catalog);
      })
      .catch((error) => {
        console.warn("Failed to refresh OpenCode model catalog", error);
      });

    return () => {
      active = false;
    };
  }, [t]);

  const selected = useMemo(
    () => models.find((model) => model.value === value),
    [models, value],
  );
  const customValue = query.trim();
  const hasExactMatch = models.some((model) => model.value === customValue);

  const save = useCallback(
    async (nextValue: string) => {
      const normalized = nextValue.trim();
      setSaving(true);
      try {
        await configApi.setOpenCodeSmallModel(normalized || null);
        setValue(normalized);
        setOpen(false);
        setQuery("");
        toast.success(t("settings.openCodeSmallModel.saved"));
      } catch (error) {
        console.error("Failed to save OpenCode small_model", error);
        toast.error(t("settings.openCodeSmallModel.saveFailed"));
      } finally {
        setSaving(false);
      }
    },
    [t],
  );

  const displayValue = selected
    ? `${selected.providerId} / ${selected.name}`
    : value || t("settings.openCodeSmallModel.useDefault");

  return (
    <SettingRow
      title={t("settings.openCodeSmallModel.title")}
      description={t("settings.openCodeSmallModel.description")}
    >
      <Popover
        open={open}
        onOpenChange={(nextOpen) => {
          setOpen(nextOpen);
          if (!nextOpen) setQuery("");
        }}
      >
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="outline"
            role="combobox"
            aria-expanded={open}
            disabled={loading || saving}
            className="h-8 w-[360px] max-w-[42vw] justify-between rounded-md px-3 text-xs font-normal shadow-none"
          >
            <span className={cn("truncate", !value && "text-muted-foreground")}>
              {loading
                ? t("settings.openCodeSmallModel.loading")
                : displayValue}
            </span>
            {loading || saving ? (
              <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin opacity-60" />
            ) : (
              <ChevronsUpDown className="h-3.5 w-3.5 shrink-0 opacity-50" />
            )}
          </Button>
        </PopoverTrigger>
        <PopoverContent
          align="end"
          className="w-[var(--radix-popover-trigger-width)] p-0"
        >
          <Command>
            <CommandInput
              value={query}
              onValueChange={setQuery}
              placeholder={t("settings.openCodeSmallModel.searchPlaceholder")}
            />
            <CommandList className="max-h-72">
              <CommandEmpty>
                {t("settings.openCodeSmallModel.noModels")}
              </CommandEmpty>
              <CommandGroup>
                <CommandItem
                  value="opencode automatic default"
                  onSelect={() => void save("")}
                >
                  <Check
                    className={cn(
                      "h-4 w-4",
                      value ? "opacity-0" : "opacity-100",
                    )}
                  />
                  {t("settings.openCodeSmallModel.useDefault")}
                </CommandItem>
                {customValue && !hasExactMatch ? (
                  <CommandItem
                    value={customValue}
                    onSelect={() => void save(customValue)}
                  >
                    <Check className="h-4 w-4 opacity-0" />
                    <span className="truncate">
                      {t("settings.openCodeSmallModel.useCustom", {
                        model: customValue,
                      })}
                    </span>
                  </CommandItem>
                ) : null}
                {models.map((model) => (
                  <CommandItem
                    key={model.value}
                    value={`${model.value} ${model.name}`}
                    onSelect={() => void save(model.value)}
                  >
                    <Check
                      className={cn(
                        "h-4 w-4",
                        value === model.value ? "opacity-100" : "opacity-0",
                      )}
                    />
                    <span className="truncate">
                      {model.providerId} / {model.name}
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>
    </SettingRow>
  );
}
