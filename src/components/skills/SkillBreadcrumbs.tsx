import { Fragment } from "react";

export interface SkillBreadcrumbItem {
  key: string;
  label: string;
  onClick?: () => void;
}

export function SkillBreadcrumbs({ items }: { items: SkillBreadcrumbItem[] }) {
  return (
    <nav
      aria-label="Breadcrumb"
      className="mb-6 flex min-w-0 items-center gap-2 overflow-hidden font-mono text-sm text-muted-foreground"
    >
      {items.map((item, index) => {
        const current = index === items.length - 1;
        return (
          <Fragment key={`${index}:${item.key}`}>
            {index > 0 && (
              <span aria-hidden="true" className="shrink-0">
                /
              </span>
            )}
            {item.onClick ? (
              <button
                type="button"
                onClick={item.onClick}
                className="min-w-0 truncate rounded-sm outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-blue-500/40"
              >
                {item.label}
              </button>
            ) : (
              <span
                className="min-w-0 truncate"
                aria-current={current ? "page" : undefined}
              >
                {item.label}
              </span>
            )}
          </Fragment>
        );
      })}
    </nav>
  );
}
