import type { ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useUpdateProviderMutation } from "@/lib/query/mutations";
import type { Provider } from "@/types";

const apiMocks = vi.hoisted(() => ({
  update: vi.fn(),
}));

const omoMocks = vi.hoisted(() => ({
  getCurrentOmoProviderId: vi.fn(),
  getCurrentOmoSlimProviderId: vi.fn(),
}));

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  providersApi: {
    update: (...args: unknown[]) => apiMocks.update(...args),
  },
  sessionsApi: {},
  settingsApi: {},
}));

vi.mock("@/lib/api/omo", () => ({
  omoApi: {
    getCurrentOmoProviderId: () => omoMocks.getCurrentOmoProviderId(),
  },
  omoSlimApi: {
    getCurrentProviderId: () => omoMocks.getCurrentOmoSlimProviderId(),
  },
}));

vi.mock("sonner", () => ({
  toast: toastMocks,
}));

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );

  return { wrapper };
}

const provider = (overrides: Partial<Provider> = {}): Provider => ({
  id: "provider-id",
  name: "Provider",
  settingsConfig: {},
  ...overrides,
});

beforeEach(() => {
  apiMocks.update.mockReset().mockResolvedValue(true);
  omoMocks.getCurrentOmoProviderId.mockReset().mockResolvedValue("");
  omoMocks.getCurrentOmoSlimProviderId.mockReset().mockResolvedValue("");
  toastMocks.success.mockReset();
  toastMocks.info.mockReset();
  toastMocks.error.mockReset();
});

describe("useUpdateProviderMutation", () => {
  it("shows only the restart notice for the active OMO Slim provider", async () => {
    omoMocks.getCurrentOmoSlimProviderId.mockResolvedValue("omo-slim-current");
    const current = provider({
      id: "omo-slim-current",
      category: "omo-slim",
    });
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useUpdateProviderMutation("opencode"), {
      wrapper,
    });

    await act(async () => result.current.mutateAsync({ provider: current }));

    expect(toastMocks.info).toHaveBeenCalledTimes(1);
    expect(toastMocks.success).not.toHaveBeenCalled();
  });

  it("keeps the normal success notice for other providers", async () => {
    const normal = provider();
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useUpdateProviderMutation("opencode"), {
      wrapper,
    });

    await act(async () => result.current.mutateAsync({ provider: normal }));

    expect(toastMocks.success).toHaveBeenCalledTimes(1);
    expect(toastMocks.info).not.toHaveBeenCalled();
  });
});
