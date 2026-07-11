import { useTranslation } from "react-i18next";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { SettingRow } from "./SettingRow";

const UPDATE_INTERVALS = [6, 24, 168] as const;

interface ModelsDevMetadataSettingsProps {
  autoUpdate: boolean;
  intervalHours: number;
  onChange: (updates: {
    modelsDevAutoUpdate?: boolean;
    modelsDevUpdateIntervalHours?: number;
  }) => void;
}

export function ModelsDevMetadataSettings({
  autoUpdate,
  intervalHours,
  onChange,
}: ModelsDevMetadataSettingsProps) {
  const { t } = useTranslation();
  const selectedInterval = UPDATE_INTERVALS.includes(
    intervalHours as (typeof UPDATE_INTERVALS)[number],
  )
    ? intervalHours
    : 24;

  return (
    <>
      <SettingRow
        title={t("settings.modelsDev.autoUpdate.title")}
        description={t("settings.modelsDev.autoUpdate.description")}
      >
        <Switch
          checked={autoUpdate}
          onCheckedChange={(enabled) =>
            onChange({ modelsDevAutoUpdate: enabled })
          }
        />
      </SettingRow>
      <SettingRow
        title={t("settings.modelsDev.interval.title")}
        description={t("settings.modelsDev.interval.description")}
      >
        <Select
          value={String(selectedInterval)}
          disabled={!autoUpdate}
          onValueChange={(value) =>
            onChange({ modelsDevUpdateIntervalHours: Number(value) })
          }
        >
          <SelectTrigger className="h-8 w-32 rounded-md border-border/80 bg-background text-xs shadow-none">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {UPDATE_INTERVALS.map((hours) => (
              <SelectItem key={hours} value={String(hours)} className="text-xs">
                {t(`settings.modelsDev.interval.options.${hours}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </SettingRow>
    </>
  );
}
