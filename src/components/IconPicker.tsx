import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ProviderIcon } from "./ProviderIcon";
import { iconList } from "@/icons/extracted";
import { searchIcons, getIconMetadata } from "@/icons/extracted/metadata";
import { cn } from "@/lib/utils";

interface IconPickerProps {
  value?: string; // 当前选中的图标
  onValueChange: (icon: string) => void; // 选择回调
  color?: string; // 预览颜色
}

const CATEGORY_TAB_ORDER = ["ai-provider", "cloud", "tool", "other"] as const;
const ALL_TAB_VALUE = "all";

const CATEGORY_LABEL_KEYS: Record<string, string> = {
  "ai-provider": "iconPicker.category.aiProvider",
  cloud: "iconPicker.category.cloud",
  tool: "iconPicker.category.tool",
  other: "iconPicker.category.other",
};

export const IconPicker: React.FC<IconPickerProps> = ({
  value,
  onValueChange,
}) => {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState("");
  const [activeCategory, setActiveCategory] = useState<string>(ALL_TAB_VALUE);

  // 按分类分组，用于分类标签页；未识别的分类归入 "other"。
  const iconsByCategory = useMemo(() => {
    const map: Record<string, string[]> = {};
    iconList.forEach((iconName) => {
      const category = getIconMetadata(iconName)?.category ?? "other";
      (map[category] ??= []).push(iconName);
    });
    return map;
  }, []);

  const availableCategories = useMemo(
    () =>
      CATEGORY_TAB_ORDER.filter((c) => (iconsByCategory[c]?.length ?? 0) > 0),
    [iconsByCategory],
  );

  const isSearching = searchQuery.trim().length > 0;

  // 搜索时跨分类展平匹配；未搜索时只展示当前分类标签页下的图标。
  const filteredIcons = useMemo(() => {
    if (isSearching) return searchIcons(searchQuery);
    if (activeCategory === ALL_TAB_VALUE) return iconList;
    return iconsByCategory[activeCategory] ?? [];
  }, [isSearching, searchQuery, activeCategory, iconsByCategory]);

  const handleTabChange = (nextCategory: string) => {
    setActiveCategory(nextCategory);
    if (searchQuery) setSearchQuery("");
  };

  return (
    <div>
      {/* 搜索栏 + 分类标签页吸顶，滚动浏览图标网格时始终可见/可操作 */}
      <div className="sticky top-0 z-10 space-y-3 bg-background px-6 py-4 border-b border-border-default">
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="text"
            placeholder={t("iconPicker.searchPlaceholder", {
              defaultValue: "输入图标名称...",
            })}
            aria-label={t("iconPicker.search", { defaultValue: "搜索图标" })}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
            autoFocus
          />
        </div>

        <div className="flex items-center justify-between gap-3">
          <Tabs value={activeCategory} onValueChange={handleTabChange}>
            <TabsList className="h-auto flex-wrap justify-start gap-1 bg-muted p-1">
              <TabsTrigger value={ALL_TAB_VALUE} className="min-w-0 px-3">
                {t("common.all", { defaultValue: "全部" })}
              </TabsTrigger>
              {availableCategories.map((cat) => (
                <TabsTrigger key={cat} value={cat} className="min-w-0 px-3">
                  {t(CATEGORY_LABEL_KEYS[cat])}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>

          <p className="flex-shrink-0 text-xs text-muted-foreground">
            {isSearching
              ? t("iconPicker.resultCount", {
                  defaultValue: "找到 {{count}} 个图标",
                  count: filteredIcons.length,
                })
              : t("iconPicker.totalCount", {
                  defaultValue: "共 {{count}} 个图标",
                  count: filteredIcons.length,
                })}
          </p>
        </div>
      </div>

      <div className="px-6 py-4">
        {filteredIcons.length === 0 ? (
          <div className="rounded-md border border-dashed border-border-default px-3 py-8 text-center text-sm text-muted-foreground">
            {t("iconPicker.noResults", { defaultValue: "未找到匹配的图标" })}
          </div>
        ) : (
          <div className="grid grid-cols-6 sm:grid-cols-8 lg:grid-cols-10 gap-2">
            {filteredIcons.map((iconName) => {
              const meta = getIconMetadata(iconName);
              const isSelected = value === iconName;

              return (
                <button
                  key={iconName}
                  type="button"
                  onClick={() => onValueChange(iconName)}
                  className={cn(
                    "flex flex-col items-center gap-1 p-3 rounded-lg",
                    "border-2 transition-colors",
                    isSelected
                      ? "border-primary bg-primary/10"
                      : "border-transparent hover:bg-accent",
                  )}
                  title={meta?.displayName || iconName}
                >
                  <ProviderIcon icon={iconName} name={iconName} size={32} />
                  <span className="text-xs text-muted-foreground truncate w-full text-center">
                    {meta?.displayName || iconName}
                  </span>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};
