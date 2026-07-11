import React, { useId, useMemo } from "react";
import grokIconSvg from "@lobehub/icons-static-svg/icons/grok.svg?raw";
import openCodeIconSvg from "@lobehub/icons-static-svg/icons/opencode.svg?raw";
import {
  getIcon,
  hasIcon,
  getIconMetadata,
  getIconUrl,
  isUrlIcon,
} from "@/icons/extracted";
import { cn } from "@/lib/utils";
import { namespaceSvgIds } from "@/lib/namespaceSvgIds";

// Keep these brand marks sourced directly from Lobe Icons instead of the
// generated local catalog, so package upgrades carry through automatically.
const LOBE_BRAND_ICON_SVGS: Record<string, string> = {
  grok: grokIconSvg,
  opencode: openCodeIconSvg,
};

interface ProviderIconProps {
  icon?: string; // 图标名称
  name: string; // 供应商名称（用于 fallback）
  color?: string; // 自定义颜色 (Deprecated, kept for compatibility but ignored for SVG)
  size?: number | string; // 尺寸
  className?: string;
  showFallback?: boolean; // 是否显示 fallback
}

export const ProviderIcon: React.FC<ProviderIconProps> = ({
  icon,
  name,
  color,
  size = 32,
  className,
  showFallback = true,
}) => {
  const iconInstanceId = useId()
    .replace(/[^a-zA-Z0-9_-]/g, "")
    .replace(/^/, "provider-icon-");

  // 获取内联 SVG 字符串
  const iconSvg = useMemo(() => {
    const lobeBrandIcon = icon
      ? LOBE_BRAND_ICON_SVGS[icon.toLowerCase()]
      : undefined;
    if (lobeBrandIcon) {
      return namespaceSvgIds(lobeBrandIcon, iconInstanceId);
    }
    if (icon && !isUrlIcon(icon) && hasIcon(icon)) {
      return namespaceSvgIds(getIcon(icon), iconInstanceId);
    }
    return "";
  }, [icon, iconInstanceId]);

  // 获取图标 URL（URL_ICONS 列表中的 SVG / 光栅图片）
  const iconUrl = useMemo(() => {
    if (icon && isUrlIcon(icon)) {
      return getIconUrl(icon);
    }
    return "";
  }, [icon]);

  // 计算尺寸样式
  const sizeStyle = useMemo(() => {
    const sizeValue = typeof size === "number" ? `${size}px` : size;
    return {
      width: sizeValue,
      height: sizeValue,
      fontSize: sizeValue,
      lineHeight: 1,
    };
  }, [size]);

  // 获取有效颜色：优先使用传入的有效 color，否则从元数据获取 defaultColor
  const effectiveColor = useMemo(() => {
    if (color && typeof color === "string" && color.trim() !== "") {
      return color;
    }
    if (icon) {
      const metadata = getIconMetadata(icon);
      if (metadata?.defaultColor && metadata.defaultColor !== "currentColor") {
        return metadata.defaultColor;
      }
    }
    return undefined;
  }, [color, icon]);

  // 内联 SVG 渲染（支持 CSS currentColor 着色）
  if (iconSvg) {
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center flex-shrink-0 [&>svg]:block",
          className,
        )}
        title={name}
        style={{ ...sizeStyle, color: effectiveColor }}
        dangerouslySetInnerHTML={{ __html: iconSvg }}
      />
    );
  }

  // URL-based 图标（大型 SVG / 光栅图片）：以 <img> 渲染
  if (iconUrl) {
    return (
      <img
        src={iconUrl}
        alt={name}
        title={name}
        className={cn(
          "inline-flex items-center justify-center flex-shrink-0 object-contain",
          className,
        )}
        style={{ width: sizeStyle.width, height: sizeStyle.height }}
        loading="lazy"
      />
    );
  }

  // Fallback：显示首字母
  if (showFallback) {
    const initials = name
      .split(" ")
      .map((word) => word[0])
      .join("")
      .toUpperCase()
      .slice(0, 2);
    const fallbackFontSize =
      typeof size === "number" ? `${Math.max(size * 0.5, 12)}px` : "0.5em";
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center flex-shrink-0 rounded-lg",
          "bg-muted text-muted-foreground font-semibold",
          className,
        )}
        title={name}
        style={sizeStyle}
      >
        <span
          style={{
            fontSize: fallbackFontSize,
          }}
        >
          {initials}
        </span>
      </span>
    );
  }

  return null;
};
