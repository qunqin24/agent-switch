import { cn } from "@/lib/utils";

interface SettingRowProps {
  title: React.ReactNode;
  description?: string;
  children: React.ReactNode;
  layout?: "horizontal" | "vertical";
  className?: string;
}

export function SettingRow({
  title,
  description,
  children,
  layout = "horizontal",
  className,
}: SettingRowProps) {
  const isVertical = layout === "vertical";

  return (
    <div
      className={cn(
        "px-4 py-3",
        isVertical ? "space-y-3" : "flex items-center justify-between gap-6",
        className,
      )}
    >
      <div className={cn("min-w-0", isVertical ? "space-y-1" : "flex-1 pr-4")}>
        <div className="text-[13px] font-medium leading-snug text-foreground">
          {title}
        </div>
        {description ? (
          <p className="text-[11px] leading-relaxed text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      <div
        className={cn(
          isVertical ? "w-full" : "flex flex-shrink-0 items-center justify-end",
        )}
      >
        {children}
      </div>
    </div>
  );
}
