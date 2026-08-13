import { render, screen } from "@testing-library/react";
import type { ComponentProps, PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { describe, expect, it, vi } from "vitest";
import { CodexFormFields } from "@/components/providers/forms/CodexFormFields";
import { Form } from "@/components/ui/form";

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

describe("CodexFormFields", () => {
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
});
