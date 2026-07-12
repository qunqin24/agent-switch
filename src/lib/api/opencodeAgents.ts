import { invoke } from "@tauri-apps/api/core";

export type OpenCodeAgentScope = "global" | "project";

export interface OpenCodeAgentDocument {
  id: string;
  scope: OpenCodeAgentScope;
  filePath: string;
  frontmatter: Record<string, unknown>;
  prompt: string;
  lastModified?: number | null;
  managedBy?: "omo-slim" | null;
}

interface AgentLocation {
  scope: OpenCodeAgentScope;
  projectDir?: string;
}

export const opencodeAgentsApi = {
  list(location: AgentLocation): Promise<OpenCodeAgentDocument[]> {
    return invoke("list_opencode_agents", {
      scope: location.scope,
      projectDir: location.projectDir,
    });
  },

  listMcpServerIds(): Promise<string[]> {
    return invoke("list_opencode_mcp_server_ids");
  },

  save(
    location: AgentLocation,
    agent: OpenCodeAgentDocument,
    originalId?: string,
  ): Promise<OpenCodeAgentDocument> {
    return invoke("save_opencode_agent", {
      ...location,
      agent,
      originalId,
    });
  },

  delete(location: AgentLocation, id: string): Promise<void> {
    return invoke("delete_opencode_agent", { ...location, id });
  },
};
