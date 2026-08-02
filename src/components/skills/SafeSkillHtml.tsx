import { Fragment, useMemo, type ReactNode } from "react";

interface SafeSkillHtmlProps {
  html: string;
  compact?: boolean;
}

function safeImageSource(value: string | null): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value, "https://www.skills.sh/");
    return url.protocol === "https:" ? url.toString() : undefined;
  } catch {
    return undefined;
  }
}

function renderNode(node: ChildNode, key: string): ReactNode {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent;
  }
  if (!(node instanceof HTMLElement)) {
    return null;
  }

  const children = Array.from(node.childNodes).map((child, index) =>
    renderNode(child, `${key}-${index}`),
  );

  switch (node.tagName.toLowerCase()) {
    case "h1":
      return (
        <h1
          key={key}
          className="mb-3 mt-8 text-3xl font-semibold tracking-tight first:mt-0"
        >
          {children}
        </h1>
      );
    case "h2":
      return (
        <h2
          key={key}
          className="mb-2.5 mt-8 text-xl font-semibold tracking-tight"
        >
          {children}
        </h2>
      );
    case "h3":
      return (
        <h3 key={key} className="mb-2 mt-6 text-base font-semibold">
          {children}
        </h3>
      );
    case "h4":
      return (
        <h4 key={key} className="mb-2 mt-5 text-sm font-semibold">
          {children}
        </h4>
      );
    case "p":
      return (
        <p key={key} className="my-3 leading-7 text-muted-foreground">
          {children}
        </p>
      );
    case "ul":
      return (
        <ul
          key={key}
          className="my-3 list-disc space-y-1.5 pl-6 text-muted-foreground"
        >
          {children}
        </ul>
      );
    case "ol":
      return (
        <ol
          key={key}
          className="my-3 list-decimal space-y-1.5 pl-6 text-muted-foreground"
        >
          {children}
        </ol>
      );
    case "li":
      return (
        <li key={key} className="pl-1 leading-7">
          {children}
        </li>
      );
    case "strong":
    case "b":
      return (
        <strong key={key} className="font-semibold text-foreground">
          {children}
        </strong>
      );
    case "em":
    case "i":
      return <em key={key}>{children}</em>;
    case "code":
      return (
        <code
          key={key}
          className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.9em] text-foreground"
        >
          {children}
        </code>
      );
    case "pre":
      return (
        <pre
          key={key}
          className="my-4 overflow-x-auto rounded-lg border border-border bg-muted/65 p-4 font-mono text-xs leading-6 text-foreground"
        >
          {children}
        </pre>
      );
    case "blockquote":
      return (
        <blockquote
          key={key}
          className="my-4 border-l-2 border-border pl-4 text-muted-foreground"
        >
          {children}
        </blockquote>
      );
    case "a":
      return (
        <span
          key={key}
          className="text-foreground underline decoration-border underline-offset-4"
        >
          {children}
        </span>
      );
    case "table":
      return (
        <div key={key} className="my-4 overflow-x-auto">
          <table className="w-full border-collapse text-sm">{children}</table>
        </div>
      );
    case "thead":
      return <thead key={key}>{children}</thead>;
    case "tbody":
      return <tbody key={key}>{children}</tbody>;
    case "tr":
      return <tr key={key}>{children}</tr>;
    case "th":
      return (
        <th
          key={key}
          className="border border-border bg-muted/50 px-3 py-2 text-left font-semibold"
        >
          {children}
        </th>
      );
    case "td":
      return (
        <td
          key={key}
          className="border border-border px-3 py-2 text-muted-foreground"
        >
          {children}
        </td>
      );
    case "hr":
      return <hr key={key} className="my-7 border-border" />;
    case "br":
      return <br key={key} />;
    case "img": {
      const src = safeImageSource(node.getAttribute("src"));
      if (!src) return node.getAttribute("alt") || null;
      return (
        <img
          key={key}
          src={src}
          alt={node.getAttribute("alt") || ""}
          className="my-5 max-h-[28rem] max-w-full rounded-lg border border-border object-contain"
        />
      );
    }
    default:
      return <Fragment key={key}>{children}</Fragment>;
  }
}

export function SafeSkillHtml({ html, compact = false }: SafeSkillHtmlProps) {
  const content = useMemo(() => {
    const document = new DOMParser().parseFromString(html, "text/html");
    return Array.from(document.body.childNodes).map((node, index) =>
      renderNode(node, String(index)),
    );
  }, [html]);

  return (
    <div className={compact ? "text-sm" : "text-sm sm:text-[15px]"}>
      {content}
    </div>
  );
}
