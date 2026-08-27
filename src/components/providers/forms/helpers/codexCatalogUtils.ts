import {
  getModelsDevReasoningEfforts,
  type ModelsDevCatalogModel,
} from "@/lib/modelsDevCatalog";

/** Same fallback triple the OpenCode thinking-level dropdown uses. */
export const DEFAULT_CODEX_REASONING_LEVELS = ["low", "medium", "high"];

/** Minimal shape the context-window backfill needs from a catalog row. */
export interface CodexCatalogContextRow {
  rowId: string;
  model: string;
  contextWindow?: string | number;
}

/** Treat undefined/null/blank as "user has not filled this in yet". */
export function isCodexContextWindowEmpty(
  value: string | number | null | undefined,
): boolean {
  if (value === null || value === undefined) return true;
  return String(value).trim() === "";
}

/**
 * Models.dev reports the context window as `limit.context`. Anything that is
 * not a usable positive integer is treated as "unknown" so the row stays empty
 * and Codex falls back to its own default.
 */
export function getModelsDevContextWindow(
  metadata: ModelsDevCatalogModel | null | undefined,
): number | undefined {
  const context = metadata?.limit?.context;
  if (
    typeof context !== "number" ||
    !Number.isFinite(context) ||
    context <= 0
  ) {
    return undefined;
  }
  return Math.trunc(context);
}

/**
 * Writes a resolved context window into one catalog row.
 *
 * Returns the original array (same reference) whenever nothing should change:
 * the row disappeared, its model id changed while the lookup was in flight, the
 * user already typed a value, or Models.dev had no context length. This keeps
 * the backfill strictly additive and safe to run from async callbacks.
 */
export function backfillCodexCatalogContextWindow<
  T extends CodexCatalogContextRow,
>(
  rows: T[],
  params: {
    rowId: string;
    model: string;
    contextWindow: number | undefined;
  },
): T[] {
  const { rowId, model, contextWindow } = params;
  if (!contextWindow) return rows;

  const index = rows.findIndex((row) => row.rowId === rowId);
  if (index === -1) return rows;

  const row = rows[index];
  // The row may have been retyped while the metadata lookup was pending.
  if (row.model.trim() !== model.trim()) return rows;
  if (!isCodexContextWindowEmpty(row.contextWindow)) return rows;

  const next = rows.slice();
  next[index] = { ...row, contextWindow: String(contextWindow) };
  return next;
}

/** Rows that can still benefit from a Models.dev lookup. */
export function selectCodexRowsNeedingContextWindow<
  T extends CodexCatalogContextRow,
>(rows: T[]): T[] {
  return rows.filter(
    (row) =>
      row.model.trim() !== "" && isCodexContextWindowEmpty(row.contextWindow),
  );
}

/** Boolean capability flags Models.dev declares, in the order OpenCode shows them. */
export const MODELS_DEV_CAPABILITY_FLAGS = [
  "attachment",
  "reasoning",
  "tool_call",
  "structured_output",
  "temperature",
] as const;

export type ModelsDevCapabilityFlag =
  (typeof MODELS_DEV_CAPABILITY_FLAGS)[number];

/**
 * The capability flags a model actually declares. The i18n labels live in the
 * caller so this stays a pure, translation-free selector.
 */
export function getModelsDevCapabilityFlags(
  metadata: ModelsDevCatalogModel | null | undefined,
): ModelsDevCapabilityFlag[] {
  if (!metadata) return [];
  return MODELS_DEV_CAPABILITY_FLAGS.filter(
    (flag) => metadata[flag] === true,
  ) as ModelsDevCapabilityFlag[];
}

/** `text/image -> text`, matching the modality line of the OpenCode panel. */
export function formatModelsDevModalities(
  metadata: ModelsDevCatalogModel | null | undefined,
): string | undefined {
  const input = metadata?.modalities?.input;
  const output = metadata?.modalities?.output;
  if (!input?.length && !output?.length) return undefined;
  return `${input?.join("/") ?? ""} -> ${output?.join("/") ?? ""}`;
}

/**
 * Reasoning levels offered for `default_reasoning_level`.
 *
 * Models.dev's explicit effort values win; otherwise we fall back to the same
 * low/medium/high triple the OpenCode panel uses, plus whatever the row already
 * stores so an imported value is never silently dropped from the dropdown.
 */
export function getCodexReasoningLevelOptions(
  metadata: ModelsDevCatalogModel | null | undefined,
  currentLevel?: string,
): string[] {
  const declared = getModelsDevReasoningEfforts(metadata);
  const base = declared.length ? declared : DEFAULT_CODEX_REASONING_LEVELS;
  const current = currentLevel?.trim();
  return [...new Set(current ? [...base, current] : base)];
}
