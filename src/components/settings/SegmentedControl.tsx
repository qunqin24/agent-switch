import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface SegmentedOption<T extends string> {
  value: T;
  label: React.ReactNode;
  ariaLabel?: string;
  icon?: React.ComponentType<{ className?: string }>;
}

interface SegmentedControlProps<T extends string> {
  value?: T;
  options: SegmentedOption<T>[];
  onChange: (value: T) => void;
  className?: string;
  disabled?: boolean;
}

export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  className,
  disabled = false,
}: SegmentedControlProps<T>) {
  return (
    <div
      className={cn(
        "inline-flex flex-shrink-0 gap-0.5 rounded-md bg-muted/70 p-0.5",
        className,
      )}
      role="group"
    >
      {options.map((option) => {
        const active = value === option.value;
        const Icon = option.icon;

        return (
          <Button
            key={option.value}
            type="button"
            variant="ghost"
            disabled={disabled}
            aria-label={option.ariaLabel}
            aria-pressed={active}
            onClick={() => onChange(option.value)}
            className={cn(
              "h-7 min-w-[68px] rounded-[6px] border-none px-2.5 text-[12px] font-medium shadow-none transition-all",
              active
                ? "bg-background text-foreground shadow-sm hover:bg-background dark:bg-blue-600 dark:text-white dark:hover:bg-blue-600"
                : "bg-transparent text-muted-foreground hover:bg-background/40 hover:text-foreground",
            )}
          >
            {Icon ? <Icon className="mr-1 h-3.5 w-3.5" /> : null}
            {option.label}
          </Button>
        );
      })}
    </div>
  );
}
