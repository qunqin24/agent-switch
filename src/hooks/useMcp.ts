import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { mcpApi } from "@/lib/api/mcp";
import type { McpAppId } from "@/lib/api/types";
import type { McpServerSpec } from "@/types";

const appMcpQueryKey = (app: McpAppId | null) => ["mcp", "app", app] as const;

export function useMcpServersForApp(app: McpAppId | null) {
  return useQuery({
    queryKey: appMcpQueryKey(app),
    queryFn: () => {
      if (app === null) {
        throw new Error("This app does not support CLI MCP management");
      }
      return mcpApi.getServersForApp(app);
    },
    enabled: app !== null,
  });
}

interface UpsertMcpServerInput {
  id: string;
  serverSpec: McpServerSpec;
}

export function useUpsertMcpServer(app: McpAppId) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, serverSpec }: UpsertMcpServerInput) =>
      mcpApi.upsertServerForApp(app, id, serverSpec),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: appMcpQueryKey(app),
      });
    },
  });
}

interface DeleteMcpServerInput {
  app: McpAppId;
  id: string;
}

export function useDeleteMcpServer() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ app, id }: DeleteMcpServerInput) =>
      mcpApi.deleteServerForApp(app, id),
    onSuccess: (_data, { app }) => {
      void queryClient.invalidateQueries({
        queryKey: appMcpQueryKey(app),
      });
    },
  });
}
