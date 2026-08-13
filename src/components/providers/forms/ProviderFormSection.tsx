import {
  useId,
  type AriaAttributes,
  type ComponentType,
  type ReactNode,
} from "react";
import { cn } from "@/lib/utils";

interface ProviderFormSectionProps {
  icon: ComponentType<{
    className?: string;
    "aria-hidden"?: AriaAttributes["aria-hidden"];
  }>;
  title: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
  className?: string;
  contentClassName?: string;
  sectionKey?: string;
}

/**
 * Shared visual grouping for provider forms. Keeping the section shell here
 * ensures every CLI uses the same spacing, border, heading, and action layout.
 */
export function ProviderFormSection({
  icon: Icon,
  title,
  children,
  actions,
  className,
  contentClassName,
  sectionKey,
}: ProviderFormSectionProps) {
  const titleId = useId();

  return (
    <section
      aria-labelledby={titleId}
      data-provider-section={sectionKey}
      className={cn(
        "rounded-lg border border-border-default bg-card p-4",
        className,
      )}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md bg-accent text-muted-foreground">
            <Icon aria-hidden={true} className="h-3.5 w-3.5" />
          </span>
          <h3 id={titleId} className="truncate text-sm font-semibold">
            {title}
          </h3>
        </div>
        {actions && <div className="flex-shrink-0">{actions}</div>}
      </div>
      <div className={cn("mt-4 space-y-4", contentClassName)}>{children}</div>
    </section>
  );
}
