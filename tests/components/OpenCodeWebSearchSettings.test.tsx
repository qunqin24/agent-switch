import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { OpenCodeWebSearchSettings } from "@/components/settings/OpenCodeWebSearchSettings";
import { server } from "../msw/server";

const toastSuccessMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: vi.fn(),
  },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

const TAURI_ENDPOINT = "http://tauri.local";

describe("OpenCodeWebSearchSettings", () => {
  it("loads the effective state and persists a toggle change", async () => {
    const writes: boolean[] = [];

    server.use(
      http.post(`${TAURI_ENDPOINT}/get_opencode_web_search_enabled`, () =>
        HttpResponse.json(true),
      ),
      http.post(
        `${TAURI_ENDPOINT}/set_opencode_web_search_enabled`,
        async ({ request }) => {
          const body = (await request.json()) as { enabled: boolean };
          writes.push(body.enabled);
          return HttpResponse.json(null);
        },
      ),
    );

    render(<OpenCodeWebSearchSettings />);

    const toggle = await screen.findByRole("switch", {
      name: "settings.openCodeWebSearch.title",
    });
    await waitFor(() => {
      expect(toggle).toBeChecked();
    });

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(writes).toEqual([false]);
      expect(toggle).not.toBeChecked();
      expect(toastSuccessMock).toHaveBeenCalledWith(
        "settings.openCodeWebSearch.disabled",
      );
    });
  });
});
