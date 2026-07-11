function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Inline SVG definition IDs are document-global. Scope them per rendered icon
 * so two copies of a gradient-based icon cannot resolve each other's fills.
 */
export function namespaceSvgIds(svg: string, prefix: string): string {
  const ids = new Map<string, string>();
  let scoped = svg.replace(/\bid=(["'])([^"']+)\1/g, (_match, quote, id) => {
    const nextId = `${prefix}-${id}`;
    ids.set(id, nextId);
    return `id=${quote}${nextId}${quote}`;
  });

  for (const [id, nextId] of ids) {
    const escapedId = escapeRegExp(id);
    scoped = scoped
      .replace(new RegExp(`url\\(#${escapedId}\\)`, "g"), `url(#${nextId})`)
      .replace(
        new RegExp(`(href|xlink:href)=(["'])#${escapedId}\\2`, "g"),
        (_match, attribute, quote) => `${attribute}=${quote}#${nextId}${quote}`,
      );
  }

  return scoped;
}
