export const MODELS_DEV_API_URL = "https://models.dev/api.json";
export const MODELS_DEV_MODELS_URL = "https://models.dev/models.json";

export interface ModelsDevCatalogModel {
  id?: string;
  name?: string;
  attachment?: boolean;
  reasoning?: boolean;
  reasoning_options?: ModelsDevReasoningOption[];
  tool_call?: boolean;
  structured_output?: boolean;
  temperature?: boolean;
  modalities?: {
    input?: string[];
    output?: string[];
  };
  limit?: {
    context?: number;
    output?: number;
  };
}

export interface ModelsDevReasoningOption {
  type?: string;
  values?: string[];
}

export interface ModelsDevCatalogProvider {
  id?: string;
  models?: Record<string, ModelsDevCatalogModel>;
}

/**
 * Models.dev's full API is provider-indexed. The optional `models` member is
 * retained for backwards-compatible tests and callers of the flat catalog.
 */
export interface ModelsDevCatalog {
  models?: Record<string, ModelsDevCatalogModel>;
  [providerId: string]:
    | ModelsDevCatalogProvider
    | Record<string, ModelsDevCatalogModel>
    | undefined;
}

export type ModelsDevModelIndex = Record<string, ModelsDevCatalogModel>;

function normalizeIdentifier(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[@_\s]+/g, "-")
    .replace(/-+/g, "-");
}

function modelSuffix(value: string): string {
  return normalizeIdentifier(value.slice(value.lastIndexOf("/") + 1));
}

function providerPrefix(value: string): string | undefined {
  const slashIndex = value.indexOf("/");
  return slashIndex > 0 ? value.slice(0, slashIndex) : undefined;
}

/**
 * models.json is Models.dev's canonical model index. It identifies the
 * official provider for a bare model ID before api.json is consulted for the
 * provider-specific options and limits.
 */
export function findOfficialModelsDevProviderId(
  modelIndex: ModelsDevModelIndex,
  modelId: string,
  displayName?: string,
): string | undefined {
  const normalizedId = normalizeIdentifier(modelId);
  const normalizedName = displayName ? normalizeIdentifier(displayName) : "";
  const idMatches = Object.keys(modelIndex).filter(
    (canonicalId) =>
      normalizeIdentifier(canonicalId) === normalizedId ||
      modelSuffix(canonicalId) === normalizedId,
  );
  if (idMatches.length === 1) return providerPrefix(idMatches[0]);

  const nameMatches = Object.entries(modelIndex)
    .filter(
      ([, model]) =>
        normalizedName && normalizeIdentifier(model.name || "") === normalizedName,
    )
    .map(([canonicalId]) => canonicalId);
  return nameMatches.length === 1 ? providerPrefix(nameMatches[0]) : undefined;
}

/**
 * Custom providers usually expose only a vendor model ID (for example
 * `glm-5.2`), while Models.dev uses the canonical `zhipuai/glm-5.2` form.
 */
export function findModelsDevCatalogModel(
  catalog: ModelsDevCatalog,
  modelId: string,
  displayName?: string,
  officialProviderId?: string,
): ModelsDevCatalogModel | undefined {
  const normalizedId = normalizeIdentifier(modelId);
  const normalizedName = displayName ? normalizeIdentifier(displayName) : "";
  if (!normalizedId && !normalizedName) return undefined;

  const entries: Array<[string, ModelsDevCatalogModel]> = Object.entries(
    catalog.models ?? {},
  );
  for (const [providerId, provider] of Object.entries(catalog)) {
    if (providerId === "models" || !provider || typeof provider !== "object") {
      continue;
    }
    const models = (provider as ModelsDevCatalogProvider).models;
    if (!models) continue;
    for (const [modelId, model] of Object.entries(models)) {
      entries.push([
        `${(provider as ModelsDevCatalogProvider).id ?? providerId}/${model.id ?? modelId}`,
        model,
      ]);
    }
  }

  const normalizedOfficialProvider = officialProviderId
    ? normalizeIdentifier(officialProviderId)
    : "";
  const idMatches: Array<[string, ModelsDevCatalogModel]> = [];
  let nameMatch: ModelsDevCatalogModel | undefined;
  let hasAmbiguousNameMatch = false;
  for (const [key, model] of entries) {
    const canonicalId = model.id || key;
    const canonicalSuffix = modelSuffix(canonicalId);
    if (
      normalizedId &&
      (normalizeIdentifier(canonicalId) === normalizedId ||
        canonicalSuffix === normalizedId)
    ) {
      idMatches.push([key, model]);
      continue;
    }

    if (
      normalizedName &&
      normalizeIdentifier(model.name || "") === normalizedName
    ) {
      if (nameMatch) {
        hasAmbiguousNameMatch = true;
      } else {
        nameMatch = model;
      }
    }
  }

  if (idMatches.length > 0) {
    if (!normalizedOfficialProvider) {
      return idMatches.length === 1 ? idMatches[0][1] : undefined;
    }
    return idMatches.find(([canonicalId]) =>
      normalizeIdentifier(providerPrefix(canonicalId) ?? "") ===
      normalizedOfficialProvider,
    )?.[1];
  }

  return hasAmbiguousNameMatch ? undefined : nameMatch;
}

/**
 * Models.dev distinguishes a model that can reason from the concrete effort
 * values its API accepts. Only the latter is safe to turn into variants.
 */
export function getModelsDevReasoningEfforts(
  model: ModelsDevCatalogModel | null | undefined,
): string[] {
  const efforts = model?.reasoning_options?.find(
    (option) => option.type === "effort",
  )?.values;
  if (!efforts) return [];

  return [...new Set(efforts.filter((value) => value.trim().length > 0))];
}

/** Only copy capability declarations explicitly present in Models.dev. */
export function getModelsDevCapabilityDeclarations(
  model: ModelsDevCatalogModel | null | undefined,
): Record<string, unknown> {
  if (!model) return {};

  const declarations: Record<string, unknown> = {};
  for (const key of [
    "attachment",
    "reasoning",
    "tool_call",
    "structured_output",
    "temperature",
  ] as const) {
    if (typeof model[key] === "boolean") declarations[key] = model[key];
  }
  if (model.modalities?.input || model.modalities?.output) {
    declarations.modalities = {
      ...(model.modalities.input ? { input: model.modalities.input } : {}),
      ...(model.modalities.output ? { output: model.modalities.output } : {}),
    };
  }
  return declarations;
}
