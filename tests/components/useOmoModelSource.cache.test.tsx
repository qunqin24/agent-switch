import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { providersApi } from "@/lib/api";
import type { OmoOpenCodeModel } from "@/lib/api/providers";
import {
  readCachedOmoModelCatalog,
  useOmoModelSource,
  writeCachedOmoModelCatalog,
} from "@/components/providers/forms/hooks/useOmoModelSource";

vi.mock("@/lib/query/queries", () => ({
  useProvidersQuery: () => ({ data: { providers: {} } }),
}));

function model(name: string): OmoOpenCodeModel {
  return {
    value: "openai/gpt-5.6-sol",
    providerId: "openai",
    modelId: "gpt-5.6-sol",
    name,
    variants: ["medium", "high"],
  };
}

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("useOmoModelSource catalog cache", () => {
  it("renders cached labels immediately and refreshes them silently", async () => {
    writeCachedOmoModelCatalog([model("Cached GPT")]);

    let resolveRefresh!: (models: OmoOpenCodeModel[]) => void;
    const pendingRefresh = new Promise<OmoOpenCodeModel[]>((resolve) => {
      resolveRefresh = resolve;
    });
    vi.spyOn(providersApi, "getOpenCodeLiveProviderIds").mockResolvedValue([]);
    vi.spyOn(providersApi, "listOpenCodeModelsForOmo").mockReturnValue(
      pendingRefresh,
    );

    const { result } = renderHook(() =>
      useOmoModelSource({ isOmoCategory: true }),
    );

    expect(result.current.isOmoModelCatalogLoading).toBe(false);
    expect(result.current.omoModelOptions[0]?.label).toContain("Cached GPT");

    await act(async () => {
      resolveRefresh([model("Fresh GPT")]);
      await pendingRefresh;
    });

    await waitFor(() => {
      expect(result.current.omoModelOptions[0]?.label).toContain("Fresh GPT");
    });
    expect(readCachedOmoModelCatalog()?.[0]?.name).toBe("Fresh GPT");
  });
});
