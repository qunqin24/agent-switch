import { useTranslation } from "react-i18next";
import { useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ArrowLeft } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ProviderIcon } from "@/components/ProviderIcon";
import { IconPicker } from "@/components/IconPicker";
import { getIconMetadata } from "@/icons/extracted/metadata";
import {
  isWindows,
  isLinux,
  DRAG_REGION_ATTR,
  DRAG_REGION_STYLE,
} from "@/lib/platform";
import type { UseFormReturn } from "react-hook-form";
import type { ProviderFormData } from "@/lib/schemas/provider";

// 与 FullScreenPanel 保持一致：macOS 上为红绿灯预留拖拽区域，Windows/Linux 上不需要。
const DRAG_BAR_HEIGHT = isWindows() || isLinux() ? 0 : 28;

interface BasicFormFieldsProps {
  form: UseFormReturn<ProviderFormData>;
  /** Slot to render content between icon and name fields */
  beforeNameSlot?: ReactNode;
}

export function BasicFormFields({
  form,
  beforeNameSlot,
}: BasicFormFieldsProps) {
  const { t } = useTranslation();
  const [iconDialogOpen, setIconDialogOpen] = useState(false);

  const currentIcon = form.watch("icon");
  const currentIconColor = form.watch("iconColor");
  const providerName = form.watch("name") || "Provider";
  const effectiveIconColor =
    currentIconColor ||
    (currentIcon ? getIconMetadata(currentIcon)?.defaultColor : undefined);

  const handleIconSelect = (icon: string) => {
    const meta = getIconMetadata(icon);
    form.setValue("icon", icon);
    form.setValue("iconColor", meta?.defaultColor ?? "");
  };

  return (
    <>
      {/* 身份信息 - 图标与名称同行，减少纵向空间占用 */}
      <div className="flex items-start gap-4">
        <Dialog open={iconDialogOpen} onOpenChange={setIconDialogOpen}>
          <DialogTrigger asChild>
            <button
              type="button"
              className="flex-shrink-0 w-14 h-14 p-2.5 rounded-xl border-2 border-muted hover:border-primary transition-colors cursor-pointer bg-muted/30 hover:bg-muted/50 flex items-center justify-center"
              title={
                currentIcon
                  ? t("providerIcon.clickToChange", {
                      defaultValue: "点击更换图标",
                    })
                  : t("providerIcon.clickToSelect", {
                      defaultValue: "点击选择图标",
                    })
              }
            >
              <ProviderIcon
                icon={currentIcon}
                name={providerName}
                color={effectiveIconColor}
                size={32}
              />
            </button>
          </DialogTrigger>
          <DialogContent
            variant="fullscreen"
            zIndex="top"
            overlayClassName="bg-[hsl(var(--background))] backdrop-blur-0"
            className="p-0 sm:rounded-none"
          >
            <div className="flex h-full flex-col">
              {DRAG_BAR_HEIGHT > 0 && (
                <div
                  {...DRAG_REGION_ATTR}
                  style={
                    {
                      ...DRAG_REGION_STYLE,
                      height: DRAG_BAR_HEIGHT,
                    } as CSSProperties
                  }
                  className="flex-shrink-0"
                />
              )}
              <div className="flex-shrink-0 py-4 border-b border-border-default">
                <div className="px-6 flex items-center gap-4">
                  <DialogClose asChild>
                    <Button type="button" variant="outline" size="icon">
                      <ArrowLeft className="h-4 w-4" />
                    </Button>
                  </DialogClose>
                  <p className="text-lg font-semibold leading-tight">
                    {t("providerIcon.selectIcon", {
                      defaultValue: "选择图标",
                    })}
                  </p>
                </div>
              </div>
              <div className="flex-1 overflow-y-auto">
                <IconPicker
                  value={currentIcon}
                  onValueChange={handleIconSelect}
                  color={effectiveIconColor}
                />
              </div>
              <div className="flex-shrink-0 py-4 border-t border-border-default">
                <div className="px-6 flex items-center justify-end gap-3">
                  <DialogClose asChild>
                    <Button type="button" variant="outline">
                      {t("common.done", { defaultValue: "完成" })}
                    </Button>
                  </DialogClose>
                </div>
              </div>
            </div>
          </DialogContent>
        </Dialog>

        <FormField
          control={form.control}
          name="name"
          render={({ field }) => (
            <FormItem className="flex-1">
              <FormLabel>{t("provider.name")}</FormLabel>
              <FormControl>
                <Input {...field} placeholder={t("provider.namePlaceholder")} />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      </div>

      {/* Slot for additional fields between icon and name */}
      {beforeNameSlot}

      {/* 基础信息 - 网格布局 */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <FormField
          control={form.control}
          name="notes"
          render={({ field }) => (
            <FormItem>
              <FormLabel>{t("provider.notes")}</FormLabel>
              <FormControl>
                <Input
                  {...field}
                  placeholder={t("provider.notesPlaceholder")}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />

        <FormField
          control={form.control}
          name="websiteUrl"
          render={({ field }) => (
            <FormItem>
              <FormLabel>{t("provider.websiteUrl")}</FormLabel>
              <FormControl>
                <Input
                  {...field}
                  placeholder={t("providerForm.websiteUrlPlaceholder")}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      </div>
    </>
  );
}
