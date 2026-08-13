import { invoke } from "@tauri-apps/api/core";
import type { ModelsDevCatalogModel } from "@/lib/modelsDevCatalog";

export interface ModelsDevCacheStatus {
  autoUpdate: boolean;
  intervalHours: number;
  fetchedAt: number | null;
  due: boolean;
  cachePath: string;
}

export interface ModelsDevCacheRefreshResult {
  outcome: "updated" | "cached" | "disabled";
  status: ModelsDevCacheStatus;
}

export const modelsDevCacheApi = {
  getModelMetadata(modelId: string, displayName?: string) {
    return invoke<ModelsDevCatalogModel | null>(
      "get_models_dev_model_metadata",
      {
        modelId,
        displayName,
      },
    );
  },

  getPricingCatalog<T>() {
    return invoke<T>("get_models_dev_api_catalog");
  },

  refresh(force = true) {
    return invoke<ModelsDevCacheRefreshResult>("refresh_models_dev_cache", {
      force,
    });
  },

  getStatus() {
    return invoke<ModelsDevCacheStatus>("get_models_dev_cache_status");
  },
};
