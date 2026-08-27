import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps, PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CodexFormFields } from "@/components/providers/forms/CodexFormFields";
import { Form } from "@/components/ui/form";

const mocks = vi.hoisted(() => ({
  fetchModelsForConfig: vi.fn(),
  getModelMetadata: vi.fn(),
  toast: {
    success: vi.fn(),
    info: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

vi.mock("sonner", () => ({ toast: mocks.toast }));

vi.mock("@/lib/api/model-fetch", () => ({
  fetchModelsForConfig: mocks.fetchModelsForConfig,
  showFetchModelsError: vi.fn(),
}));

vi.mock("@/lib/api/modelsDev", () => ({
  modelsDevCacheApi: { getModelMetadata: mocks.getModelMetadata },
}));

type CodexFormFieldsProps = ComponentProps<typeof CodexFormFields>;

const FormShell = ({ children }: PropsWithChildren) => {
  const form = useForm();
  return <Form {...form}>{children}</Form>;
};

const renderForm = (overrides: Partial<CodexFormFieldsProps> = {}) => {
  const props: CodexFormFieldsProps = {
    codexApiKey: "sk-test",
    onApiKeyChange: vi.fn(),
    category: "cn_official",
    shouldShowApiKeyLink: false,
    websiteUrl: "https://platform.deepseek.com",
    shouldShowSpeedTest: false,
    codexBaseUrl: "https://api.deepseek.com/",
    onBaseUrlChange: vi.fn(),
    isFullUrl: false,
    onFullUrlChange: vi.fn(),
    isEndpointModalOpen: false,
    onEndpointModalToggle: vi.fn(),
    onCustomEndpointsChange: vi.fn(),
    autoSelect: false,
    onAutoSelectChange: vi.fn(),
    apiFormat: "openai_responses",
    onApiFormatChange: vi.fn(),
    onCodexChatReasoningChange: vi.fn(),
    catalogModels: [
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek-V4-Flash",
        contextWindow: 1048576,
      },
    ],
    onCatalogModelsChange: vi.fn(),
    speedTestEndpoints: [],
    customUserAgent: "",
    onCustomUserAgentChange: vi.fn(),
    ...overrides,
  };

  return render(
    <FormShell>
      <CodexFormFields {...props} />
    </FormShell>,
  );
};

const contextWindowInput = () =>
  screen.getByLabelText("上下文窗口") as HTMLInputElement;

const clickFetchModels = () => {
  fireEvent.click(screen.getByText("providerForm.fetchModels"));
};

const autoConfigureButtons = () => screen.getAllByText("自动配置");

// 工具栏按钮永远是第一个；行内按钮只有展开该行后才存在
const clickAutoConfigure = () => {
  fireEvent.click(autoConfigureButtons()[0]);
};

const expandRow = (index = 0) => {
  fireEvent.click(screen.getAllByTitle("模型详情")[index]);
};

describe("CodexFormFields", () => {
  beforeEach(() => {
    mocks.fetchModelsForConfig.mockReset();
    mocks.getModelMetadata.mockReset();
    mocks.toast.success.mockReset();
    mocks.toast.info.mockReset();
    mocks.toast.error.mockReset();
    mocks.toast.warning.mockReset();
    mocks.fetchModelsForConfig.mockResolvedValue([
      { id: "glm-5.2", ownedBy: null },
    ]);
    mocks.getModelMetadata.mockResolvedValue(null);
  });

  it("shows the model catalog for native Responses providers", () => {
    renderForm();

    expect(screen.getByText("模型映射")).toBeVisible();
    expect(screen.getByDisplayValue("deepseek-v4-flash")).toBeVisible();
    expect(screen.getByDisplayValue("1048576")).toBeVisible();
    expect(
      screen.getByPlaceholderText(
        "只需要填这里，启用后会自动写入 config.toml 的 experimental_bearer_token",
      ),
    ).toHaveValue("sk-test");
  });

  it("backfills the context window from Models.dev after fetching models", async () => {
    const onCatalogModelsChange = vi.fn();
    mocks.getModelMetadata.mockResolvedValue({
      id: "glm-5.2",
      limit: { context: 204800, output: 131072 },
    });

    renderForm({
      catalogModels: [{ model: "glm-5.2", displayName: "GLM 5.2" }],
      onCatalogModelsChange,
    });

    expect(contextWindowInput()).toHaveValue(null);
    clickFetchModels();

    await waitFor(() => expect(contextWindowInput()).toHaveValue(204800));
    expect(mocks.getModelMetadata).toHaveBeenCalledWith("glm-5.2", "GLM 5.2");
    await waitFor(() =>
      expect(onCatalogModelsChange).toHaveBeenCalledWith([
        {
          model: "glm-5.2",
          displayName: "GLM 5.2",
          contextWindow: "204800",
          defaultReasoningLevel: "",
        },
      ]),
    );
  });

  it("keeps a context window the user already filled in", async () => {
    const onCatalogModelsChange = vi.fn();
    mocks.getModelMetadata.mockResolvedValue({
      id: "glm-5.2",
      limit: { context: 204800 },
    });

    renderForm({
      catalogModels: [
        { model: "glm-5.2", displayName: "GLM 5.2", contextWindow: 65536 },
      ],
      onCatalogModelsChange,
    });

    clickFetchModels();

    await waitFor(() =>
      expect(mocks.fetchModelsForConfig).toHaveBeenCalledTimes(1),
    );
    expect(contextWindowInput()).toHaveValue(65536);
    // 已填值的行不需要查询元数据，也不会回传新数据
    expect(mocks.getModelMetadata).not.toHaveBeenCalled();
    expect(onCatalogModelsChange).not.toHaveBeenCalled();
  });

  it("leaves the context window empty when Models.dev has no metadata", async () => {
    const onCatalogModelsChange = vi.fn();
    mocks.getModelMetadata.mockResolvedValue(null);

    renderForm({
      catalogModels: [{ model: "glm-5.2", displayName: "GLM 5.2" }],
      onCatalogModelsChange,
    });

    clickFetchModels();

    await waitFor(() =>
      expect(mocks.getModelMetadata).toHaveBeenCalledTimes(1),
    );
    expect(contextWindowInput()).toHaveValue(null);
    expect(onCatalogModelsChange).not.toHaveBeenCalled();
  });

  it("degrades silently when the Models.dev lookup fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mocks.getModelMetadata.mockRejectedValue(new Error("cache unavailable"));

    renderForm({ catalogModels: [{ model: "glm-5.2" }] });

    clickFetchModels();

    await waitFor(() =>
      expect(mocks.getModelMetadata).toHaveBeenCalledTimes(1),
    );
    expect(contextWindowInput()).toHaveValue(null);
    // 模型列表本身仍然可用
    expect(screen.getByDisplayValue("glm-5.2")).toBeVisible();
    warn.mockRestore();
  });

  describe("auto configure button", () => {
    it("re-queries Models.dev, bypassing the in-memory cache", async () => {
      const onCatalogModelsChange = vi.fn();
      // 第一次查询没有结果（会被组件内缓存记住）
      mocks.getModelMetadata.mockResolvedValue(null);

      renderForm({
        catalogModels: [{ model: "glm-5.2", displayName: "GLM 5.2" }],
        onCatalogModelsChange,
      });

      clickAutoConfigure();
      await waitFor(() =>
        expect(mocks.getModelMetadata).toHaveBeenCalledTimes(1),
      );
      expect(contextWindowInput()).toHaveValue(null);
      expect(mocks.toast.info).toHaveBeenCalledWith(
        "Models.dev 未提供这些模型的上下文长度，请手动填写",
      );

      // 用户在设置里刷新了 models.dev 缓存后，再点一次必须重新查询
      mocks.getModelMetadata.mockResolvedValue({
        id: "glm-5.2",
        limit: { context: 204800 },
      });
      clickAutoConfigure();

      await waitFor(() => expect(contextWindowInput()).toHaveValue(204800));
      expect(mocks.getModelMetadata).toHaveBeenCalledTimes(2);
      expect(mocks.getModelMetadata).toHaveBeenLastCalledWith(
        "glm-5.2",
        "GLM 5.2",
      );
      expect(mocks.toast.success).toHaveBeenCalledWith(
        "已按 Models.dev 回填 1 个模型的上下文窗口",
      );
      await waitFor(() =>
        expect(onCatalogModelsChange).toHaveBeenCalledWith([
          {
            model: "glm-5.2",
            displayName: "GLM 5.2",
            contextWindow: "204800",
            defaultReasoningLevel: "",
          },
        ]),
      );
    });

    it("fills every empty row and leaves filled rows untouched", async () => {
      mocks.getModelMetadata.mockImplementation(async (modelId: string) =>
        modelId === "kimi-k2" ? { limit: { context: 262144 } } : null,
      );

      renderForm({
        catalogModels: [
          { model: "glm-5.2", displayName: "GLM 5.2", contextWindow: 65536 },
          { model: "kimi-k2", displayName: "Kimi K2" },
          { model: "unknown-model", displayName: "Unknown" },
        ],
      });

      clickAutoConfigure();

      await waitFor(() =>
        expect(mocks.getModelMetadata).toHaveBeenCalledTimes(2),
      );
      const inputs = screen.getAllByLabelText(
        "上下文窗口",
      ) as HTMLInputElement[];
      expect(inputs[0]).toHaveValue(65536);
      await waitFor(() => expect(inputs[1]).toHaveValue(262144));
      expect(inputs[2]).toHaveValue(null);
      // 已填的行不会被重新查询
      expect(mocks.getModelMetadata).not.toHaveBeenCalledWith(
        "glm-5.2",
        "GLM 5.2",
      );
      expect(mocks.toast.success).toHaveBeenCalledWith(
        "已按 Models.dev 回填 1 个模型的上下文窗口",
      );
    });

    it("fills only its own row when triggered from the expanded panel", async () => {
      mocks.getModelMetadata.mockImplementation(async (modelId: string) =>
        modelId === "kimi-k2" ? { limit: { context: 262144 } } : null,
      );

      renderForm({
        catalogModels: [
          { model: "glm-5.2", displayName: "GLM 5.2" },
          { model: "kimi-k2", displayName: "Kimi K2" },
        ],
      });

      expandRow(1);
      await waitFor(() =>
        expect(mocks.getModelMetadata).toHaveBeenCalledTimes(1),
      );

      fireEvent.click(autoConfigureButtons()[1]);

      await waitFor(() =>
        expect(mocks.toast.success).toHaveBeenCalledWith(
          "已按 Models.dev 回填 1 个模型的上下文窗口",
        ),
      );
      const inputs = screen.getAllByLabelText(
        "上下文窗口",
      ) as HTMLInputElement[];
      expect(inputs[0]).toHaveValue(null);
      expect(inputs[1]).toHaveValue(262144);
      // 展开时查一次 + 自动配置强制重查一次，且都只针对该行
      expect(mocks.getModelMetadata).toHaveBeenCalledTimes(2);
      expect(mocks.getModelMetadata).toHaveBeenLastCalledWith(
        "kimi-k2",
        "Kimi K2",
      );
    });
  });

  describe("row details panel", () => {
    it("shows the Models.dev capability summary when the row is expanded", async () => {
      mocks.getModelMetadata.mockResolvedValue({
        id: "glm-5.2",
        reasoning: true,
        tool_call: true,
        temperature: false,
        modalities: { input: ["text", "image"], output: ["text"] },
        limit: { context: 204800 },
      });

      renderForm({
        catalogModels: [{ model: "glm-5.2", displayName: "GLM 5.2" }],
      });

      expect(screen.queryByText("Models.dev 元数据")).toBeNull();
      expandRow();

      expect(screen.getByText("Models.dev 元数据")).toBeVisible();
      await waitFor(() =>
        expect(screen.getByText("Models.dev：支持思考")).toBeVisible(),
      );
      expect(
        screen.getByText("能力：思考 · 工具调用 · text/image -> text"),
      ).toBeVisible();
      // 展开只读取元数据展示，不改动用户数据；回填由「自动配置」触发
      expect(contextWindowInput()).toHaveValue(null);
      fireEvent.click(autoConfigureButtons()[1]);
      await waitFor(() => expect(contextWindowInput()).toHaveValue(204800));
    });

    it("reports a missing Models.dev record instead of failing", async () => {
      mocks.getModelMetadata.mockResolvedValue(null);

      renderForm({
        catalogModels: [{ model: "glm-5.2", displayName: "GLM 5.2" }],
      });

      expandRow();

      await waitFor(() =>
        expect(
          screen.getByText(
            "Models.dev 中未找到 glm-5.2；请检查模型 ID 或手动配置",
          ),
        ).toBeVisible(),
      );
      expect(contextWindowInput()).toHaveValue(null);
    });

    it("renders the stored default reasoning level", async () => {
      mocks.getModelMetadata.mockResolvedValue(null);

      renderForm({
        catalogModels: [
          {
            model: "glm-5.2",
            displayName: "GLM 5.2",
            contextWindow: 204800,
            defaultReasoningLevel: "high",
          },
        ],
      });

      expandRow();

      expect(
        screen.getByLabelText("默认思考等级") as HTMLElement,
      ).toHaveTextContent("high");
      // 面板里的上下文输入与表格列同源
      expect(screen.getByLabelText("上下文")).toHaveValue(204800);
    });
  });

  describe("more toolbar cases", () => {
    it("does nothing when there is no row left to fill", async () => {
      const onCatalogModelsChange = vi.fn();
      renderForm({
        catalogModels: [
          { model: "glm-5.2", displayName: "GLM 5.2", contextWindow: 65536 },
          { model: "", displayName: "空行" },
        ],
        onCatalogModelsChange,
      });

      clickAutoConfigure();

      await waitFor(() =>
        expect(mocks.toast.info).toHaveBeenCalledWith(
          "没有需要回填的模型：请先填写实际请求模型",
        ),
      );
      expect(mocks.getModelMetadata).not.toHaveBeenCalled();
      expect(onCatalogModelsChange).not.toHaveBeenCalled();
    });
  });
});
