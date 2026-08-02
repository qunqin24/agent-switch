import type { ReactElement } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import McpPanel from "@/components/mcp/McpPanel";
import type { McpAppId } from "@/lib/api/types";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

interface FormMockProps {
  appId: McpAppId;
  editingId?: string;
}

vi.mock("@/components/mcp/McpFormModal", () => ({
  default: ({ appId, editingId }: FormMockProps) => (
    <div data-testid="mcp-form">
      {appId}:{editingId ?? "new"}
    </div>
  ),
}));

function renderWithQueryClient(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

describe("McpPanel", () => {
  it("switches between isolated live configurations for each CLI", async () => {
    const requestedApps: string[] = [];

    server.use(
      http.post(
        `${TAURI_ENDPOINT}/get_mcp_servers_for_app`,
        async ({ request }) => {
          const body: unknown = await request.json();
          const app =
            typeof body === "object" &&
            body !== null &&
            "app" in body &&
            typeof body.app === "string"
              ? body.app
              : "unknown";
          requestedApps.push(app);

          return HttpResponse.json({
            configPath: `/mock/${app}.config`,
            storageFormat: app === "codex" ? "toml" : "json",
            servers:
              app === "codex"
                ? {
                    "codex-only": {
                      type: "stdio",
                      command: "codex-command",
                    },
                  }
                : {
                    "gemini-only": {
                      type: "stdio",
                      command: "gemini-command",
                    },
                  },
          });
        },
      ),
    );

    const view = renderWithQueryClient(<McpPanel appId="codex" />);
    expect(await screen.findByText("codex-only")).toBeInTheDocument();
    expect(screen.queryAllByRole("switch")).toHaveLength(0);

    view.rerender(
      <QueryClientProvider
        client={
          new QueryClient({
            defaultOptions: { queries: { retry: false } },
          })
        }
      >
        <McpPanel appId="gemini" />
      </QueryClientProvider>,
    );

    expect(await screen.findByText("gemini-only")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("codex-only")).not.toBeInTheDocument(),
    );
    expect(requestedApps).toEqual(["codex", "gemini"]);
  });

  it("opens an editor scoped to the selected CLI", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_mcp_servers_for_app`, () =>
        HttpResponse.json({
          configPath: "/mock/codex.toml",
          storageFormat: "toml",
          servers: {
            shared: { type: "stdio", command: "codex-command" },
          },
        }),
      ),
    );

    renderWithQueryClient(<McpPanel appId="codex" />);
    await screen.findByText("shared");

    fireEvent.click(screen.getByTitle("common.edit"));

    expect(screen.getByTestId("mcp-form")).toHaveTextContent("codex:shared");
  });
});
