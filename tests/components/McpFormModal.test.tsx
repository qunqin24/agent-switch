import React from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import McpFormModal, {
  type AppMcpServerEntry,
} from "@/components/mcp/McpFormModal";
import type { McpServerSpec } from "@/types";

const toastErrorMock = vi.hoisted(() => vi.fn());
const toastSuccessMock = vi.hoisted(() => vi.fn());
const upsertMock = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock("sonner", () => ({
  toast: {
    error: (...args: unknown[]) => toastErrorMock(...args),
    success: (...args: unknown[]) => toastSuccessMock(...args),
  },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
  initReactI18next: { type: "3rdParty", init: () => {} },
}));

vi.mock("@/config/mcpPresets", () => ({
  mcpPresets: [
    {
      id: "preset-stdio",
      server: { type: "stdio", command: "preset-cmd" },
    },
  ],
  getMcpPresetWithDescription: (preset: {
    id: string;
    server: McpServerSpec;
  }) => preset,
}));

interface JsonEditorMockProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

vi.mock("@/components/JsonEditor", () => ({
  default: ({ value, onChange, placeholder }: JsonEditorMockProps) => (
    <textarea
      value={value}
      placeholder={placeholder}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

interface WizardMockProps {
  isOpen: boolean;
  onApply: (title: string, config: string) => void;
}

vi.mock("@/components/mcp/McpWizardModal", () => ({
  default: ({ isOpen, onApply }: WizardMockProps) =>
    isOpen ? (
      <button
        type="button"
        data-testid="wizard-apply"
        onClick={() =>
          onApply(
            "wizard-id",
            JSON.stringify({ type: "stdio", command: "wizard-cmd" }),
          )
        }
      >
        wizard-apply
      </button>
    ) : null,
}));

vi.mock("@/hooks/useMcp", async () => {
  const actual =
    await vi.importActual<typeof import("@/hooks/useMcp")>("@/hooks/useMcp");
  return {
    ...actual,
    useUpsertMcpServer: () => ({
      mutateAsync: (...args: unknown[]) => upsertMock(...args),
    }),
  };
});

describe("McpFormModal", () => {
  beforeEach(() => {
    toastErrorMock.mockClear();
    toastSuccessMock.mockClear();
    upsertMock.mockClear();
    upsertMock.mockResolvedValue(undefined);
  });

  const renderForm = (
    props?: Partial<React.ComponentProps<typeof McpFormModal>>,
  ) => {
    const {
      onSave: overrideOnSave,
      onClose: overrideOnClose,
      ...rest
    } = props ?? {};
    const onSave = overrideOnSave ?? vi.fn().mockResolvedValue(undefined);
    const onClose = overrideOnClose ?? vi.fn();
    render(
      <McpFormModal
        appId="claude"
        onSave={onSave}
        onClose={onClose}
        existingIds={[]}
        defaultFormat="json"
        {...rest}
      />,
    );
    return { onSave, onClose };
  };

  it("应用预设后填充 ID 与配置内容", async () => {
    renderForm();
    fireEvent.click(await screen.findByText("preset-stdio"));

    expect(
      screen.getByPlaceholderText<HTMLInputElement>("mcp.form.titlePlaceholder")
        .value,
    ).toBe("preset-stdio");
    expect(
      screen.getByPlaceholderText<HTMLTextAreaElement>(
        "mcp.form.jsonPlaceholder",
      ).value,
    ).toBe('{\n  "type": "stdio",\n  "command": "preset-cmd"\n}');
  });

  it("提交时只保存当前 CLI 的 ID 与服务器配置", async () => {
    const { onSave } = renderForm();

    fireEvent.change(screen.getByPlaceholderText("mcp.form.titlePlaceholder"), {
      target: { value: " my-server " },
    });
    fireEvent.change(screen.getByPlaceholderText("mcp.form.jsonPlaceholder"), {
      target: { value: '{"type":"stdio","command":"run"}' },
    });
    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() => expect(upsertMock).toHaveBeenCalledTimes(1));
    expect(upsertMock).toHaveBeenCalledWith({
      id: "my-server",
      serverSpec: {
        type: "stdio",
        command: "run",
      },
    });
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(toastErrorMock).not.toHaveBeenCalled();
  });

  it("缺少配置命令时阻止提交并提示错误", async () => {
    renderForm();
    fireEvent.change(screen.getByPlaceholderText("mcp.form.titlePlaceholder"), {
      target: { value: "no-command" },
    });
    fireEvent.change(screen.getByPlaceholderText("mcp.form.jsonPlaceholder"), {
      target: { value: '{"type":"stdio"}' },
    });
    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() =>
      expect(toastErrorMock).toHaveBeenCalledWith("mcp.error.commandRequired", {
        duration: 3000,
      }),
    );
    expect(upsertMock).not.toHaveBeenCalled();
  });

  it("支持向导生成配置并自动填充 ID", async () => {
    renderForm();
    fireEvent.click(screen.getByText("mcp.form.useWizard"));

    await act(async () => {
      fireEvent.click(await screen.findByTestId("wizard-apply"));
    });

    expect(
      screen.getByPlaceholderText<HTMLInputElement>("mcp.form.titlePlaceholder")
        .value,
    ).toBe("wizard-id");
  });

  it("Codex TOML 模式下自动提取 ID 并成功保存", async () => {
    const { onSave } = renderForm({
      appId: "codex",
      defaultFormat: "toml",
    });
    const config = `[mcp_servers.demo]
type = "stdio"
command = "run"
`;

    fireEvent.change(screen.getByPlaceholderText("mcp.form.tomlPlaceholder"), {
      target: { value: config },
    });
    await waitFor(() =>
      expect(
        screen.getByPlaceholderText<HTMLInputElement>(
          "mcp.form.titlePlaceholder",
        ).value,
      ).toBe("demo"),
    );
    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() => expect(upsertMock).toHaveBeenCalledTimes(1));
    expect(upsertMock).toHaveBeenCalledWith({
      id: "demo",
      serverSpec: { type: "stdio", command: "run" },
    });
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("编辑模式下保持 ID 并更新配置", async () => {
    const initialData: AppMcpServerEntry = {
      id: "existing",
      server: { type: "stdio", command: "old" },
    };
    const { onSave } = renderForm({
      editingId: "existing",
      initialData,
    });

    const idInput = screen.getByPlaceholderText<HTMLInputElement>(
      "mcp.form.titlePlaceholder",
    );
    expect(idInput.value).toBe("existing");
    expect(idInput).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText("mcp.form.jsonPlaceholder"), {
      target: { value: '{"type":"stdio","command":"updated"}' },
    });
    fireEvent.click(screen.getByText("common.save"));

    await waitFor(() =>
      expect(upsertMock).toHaveBeenCalledWith({
        id: "existing",
        serverSpec: { type: "stdio", command: "updated" },
      }),
    );
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("保存后的关闭流程失败时展示错误并恢复按钮", async () => {
    const failingSave = vi.fn().mockRejectedValue(new Error("保存失败"));
    renderForm({ onSave: failingSave });

    fireEvent.change(screen.getByPlaceholderText("mcp.form.titlePlaceholder"), {
      target: { value: "will-fail" },
    });
    fireEvent.change(screen.getByPlaceholderText("mcp.form.jsonPlaceholder"), {
      target: { value: '{"type":"stdio","command":"ok"}' },
    });
    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() => expect(failingSave).toHaveBeenCalled());
    await waitFor(() => expect(toastErrorMock).toHaveBeenCalled());
    expect(toastErrorMock.mock.calls.at(-1)?.[0]).toBe("保存失败");
    expect(
      screen.getByText<HTMLButtonElement>("common.add"),
    ).not.toBeDisabled();
  });
});
