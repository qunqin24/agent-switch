import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { OmoFormFields } from "@/components/providers/forms/OmoFormFields";
import { omoSlimApi } from "@/lib/api/omo";
import type { OmoLocalFileData } from "@/types/omo";

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

describe("OmoFormFields optional agents", () => {
  it("expands the optional agents and other fields sections", () => {
    const queryClient = createQueryClient();

    render(
      <QueryClientProvider client={queryClient}>
        <OmoFormFields
          isSlim
          modelOptions={[]}
          agents={{}}
          onAgentsChange={vi.fn()}
          otherFieldsStr={JSON.stringify({
            disabled_agents: ["observer", "council"],
          })}
          onOtherFieldsStrChange={vi.fn()}
        />
      </QueryClientProvider>,
    );

    const trigger = screen.getByRole("button", {
      name: /Optional \/ Internal Agents/i,
    });

    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Observer")).not.toBeInTheDocument();

    fireEvent.click(trigger);

    expect(
      screen.getByRole("button", {
        name: /Optional \/ Internal Agents/i,
      }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Observer")).toBeInTheDocument();
    expect(screen.getByText("Council")).toBeInTheDocument();
    expect(screen.getByText("Councillor")).toBeInTheDocument();

    const otherFieldsTrigger = screen.getByRole("button", {
      name: /Other Fields \(JSON\)/i,
    });
    expect(otherFieldsTrigger).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(otherFieldsTrigger);

    expect(
      screen.getByRole("button", { name: /Other Fields \(JSON\)/i }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByPlaceholderText('{ "custom_key": "value" }'),
    ).toBeInTheDocument();
  });

  it("does not show the manual import spinner during automatic local sync", async () => {
    let resolveRead!: (value: OmoLocalFileData) => void;
    const pendingRead = new Promise<OmoLocalFileData>((resolve) => {
      resolveRead = resolve;
    });
    const readSpy = vi
      .spyOn(omoSlimApi, "readLocalFile")
      .mockReturnValue(pendingRead);

    const { container } = render(
      <QueryClientProvider client={createQueryClient()}>
        <OmoFormFields
          isSlim
          syncCurrentLocalFile
          modelOptions={[]}
          agents={{}}
          onAgentsChange={vi.fn()}
          otherFieldsStr=""
          onOtherFieldsStrChange={vi.fn()}
        />
      </QueryClientProvider>,
    );

    const importButton = screen.getByRole("button", { name: /Import Local/i });
    expect(importButton).toBeEnabled();
    expect(container.querySelector(".animate-spin")).not.toBeInTheDocument();

    await act(async () => {
      resolveRead({
        agents: {},
        otherFields: {},
        filePath: "/tmp/oh-my-opencode-slim.json",
      });
      await pendingRead;
    });

    await waitFor(() => {
      expect(screen.getByText("oh-my-opencode-slim.json")).toBeInTheDocument();
    });
    readSpy.mockRestore();
  });

  it("uses stable model placeholders while the catalog loads", () => {
    render(
      <QueryClientProvider client={createQueryClient()}>
        <OmoFormFields
          isSlim
          modelCatalogLoading
          modelOptions={[]}
          modelVariantsMap={{}}
          agents={{
            orchestrator: {
              model: "openai/gpt-5.6-sol",
              variant: "medium",
            },
          }}
          onAgentsChange={vi.fn()}
          otherFieldsStr=""
          onOtherFieldsStrChange={vi.fn()}
        />
      </QueryClientProvider>,
    );

    expect(
      screen.getAllByRole("status", { name: /Loading/i }),
    ).not.toHaveLength(0);
    expect(screen.queryByText("openai/gpt-5.6-sol")).not.toBeInTheDocument();
    expect(screen.getByText("medium")).toBeInTheDocument();
    expect(screen.queryByText(/current value/i)).not.toBeInTheDocument();
  });
});
