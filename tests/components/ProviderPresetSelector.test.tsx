import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { TFunction } from "i18next";
import { useForm } from "react-hook-form";
import { Form } from "@/components/ui/form";
import type { ProviderCategory } from "@/types";
import {
  ProviderPresetSelector,
  filterPresetEntries,
  getPresetDisplayName,
  getPresetSearchText,
  getVisiblePresetEntries,
  sortPresetEntries,
  type PresetSortMode,
} from "@/components/providers/forms/ProviderPresetSelector";

// Mock ProviderIcon 以避免依赖图标库的实际内容
vi.mock("@/components/ProviderIcon", () => ({
  ProviderIcon: ({
    icon,
    name,
    color,
    size,
  }: {
    icon?: string;
    name: string;
    color?: string;
    size?: number;
  }) => (
    <span
      data-testid="provider-icon"
      data-icon={icon}
      data-name={name}
      data-color={color}
      data-size={size}
    />
  ),
}));

const presetCategoryLabels = {
  official: "官方",
  cn_official: "国产官方",
  aggregator: "聚合服务",
  third_party: "第三方",
};

const translations: Record<string, string> = {
  "preset.alpha": "Alpha 本地名",
  "preset.gamma": "Gamma 本地名",
};

const t = ((key: string) => translations[key] ?? key) as TFunction;

type TestPresetEntry = {
  id: string;
  preset: {
    name: string;
    nameKey?: string;
    websiteUrl: string;
    settingsConfig: Record<string, never>;
    category: ProviderCategory;
  };
};

// 四个分类各一条，用于验证「默认只展示第一个可用分类」以及「切换标签页」的行为。
const presetEntries: TestPresetEntry[] = [
  {
    id: "gamma",
    preset: {
      name: "Gamma Raw",
      nameKey: "preset.gamma",
      websiteUrl: "https://gamma.example.com",
      settingsConfig: {},
      category: "aggregator",
    },
  },
  {
    id: "alpha",
    preset: {
      name: "Alpha Raw",
      nameKey: "preset.alpha",
      websiteUrl: "https://alpha.example.com/v1",
      settingsConfig: {},
      category: "official",
    },
  },
  {
    id: "beta",
    preset: {
      name: "Beta Gateway",
      websiteUrl: "https://CN-Gateway.example.com",
      settingsConfig: {},
      category: "cn_official",
    },
  },
  {
    id: "delta",
    preset: {
      name: "Delta Mirror",
      websiteUrl: "https://delta.example.com",
      settingsConfig: {},
      category: "third_party",
    },
  },
] satisfies TestPresetEntry[];

// 同一分类下的多条预设，用于验证单个标签页内部的排序/原始顺序行为。
const sameCategoryEntries: TestPresetEntry[] = [
  {
    id: "c-gamma",
    preset: {
      name: "Gamma Raw",
      nameKey: "preset.gamma",
      websiteUrl: "https://gamma.example.com",
      settingsConfig: {},
      category: "official",
    },
  },
  {
    id: "c-alpha",
    preset: {
      name: "Alpha Raw",
      nameKey: "preset.alpha",
      websiteUrl: "https://alpha.example.com/v1",
      settingsConfig: {},
      category: "official",
    },
  },
  {
    id: "c-beta",
    preset: {
      name: "Beta Gateway",
      websiteUrl: "https://CN-Gateway.example.com",
      settingsConfig: {},
      category: "official",
    },
  },
] satisfies TestPresetEntry[];

function getIds(entries: ReadonlyArray<{ id: string }>) {
  return entries.map((entry) => entry.id);
}

function renderSelector({
  entries = presetEntries,
  onPresetChange = vi.fn(),
}: {
  entries?: TestPresetEntry[];
  onPresetChange?: (value: string) => void;
} = {}) {
  const Wrapper = () => {
    const form = useForm();

    return (
      <Form {...form}>
        <ProviderPresetSelector
          selectedPresetId="custom"
          presetEntries={entries}
          presetCategoryLabels={presetCategoryLabels}
          onPresetChange={onPresetChange}
        />
      </Form>
    );
  };

  return render(<Wrapper />);
}

function getPresetButtonTexts() {
  const knownNames = new Set([
    ...presetEntries.flatMap((entry) => [
      entry.preset.name,
      entry.preset.nameKey ?? entry.preset.name,
    ]),
    ...sameCategoryEntries.flatMap((entry) => [
      entry.preset.name,
      entry.preset.nameKey ?? entry.preset.name,
    ]),
  ]);

  return screen
    .getAllByRole("button")
    .map((button) => button.textContent?.trim() ?? "")
    .filter((text) => knownNames.has(text));
}

function getTabTexts() {
  return screen.getAllByRole("tab").map((tab) => tab.textContent?.trim() ?? "");
}

function getSearchButton() {
  return screen.getByRole("button", {
    name: /providerPreset\.(search|searchAriaLabel|openSearch)|搜索|search/i,
  });
}

function getSortButton() {
  return screen.getByRole("button", {
    name: /providerPreset\.(sort|sortByName|restoreOriginalOrder)|按名称排序|恢复原顺序|sort/i,
  });
}

function getSearchInput() {
  return screen.getByRole("textbox", {
    name: /providerPreset\.(searchInput|searchPlaceholder)|搜索预设|search/i,
  });
}

describe("ProviderPresetSelector pure helpers", () => {
  it("优先使用 nameKey 翻译作为显示名，否则使用原始 name", () => {
    expect(getPresetDisplayName(presetEntries[1].preset, t)).toBe(
      "Alpha 本地名",
    );
    expect(getPresetDisplayName(presetEntries[2].preset, t)).toBe(
      "Beta Gateway",
    );
  });

  it("仅拼接显示名与原始名称、统一 lower-case，不含 URL 或分类 label", () => {
    const searchText = getPresetSearchText(presetEntries[1], t);

    expect(searchText).toContain("alpha 本地名");
    expect(searchText).toContain("alpha raw");
    expect(searchText).not.toContain("example.com");
    expect(searchText).not.toContain("官方");
    expect(searchText).toBe(searchText.toLowerCase());
  });

  it("空 query 返回原数组，非空 query 大小写不敏感匹配", () => {
    expect(filterPresetEntries(presetEntries, "   ", t)).toBe(presetEntries);
    expect(
      getIds(filterPresetEntries(presetEntries, "ALPHA 本地名", t)),
    ).toEqual(["alpha"]);
  });

  it("不再通过 URL 或分类 label 搜索（仅匹配名称）", () => {
    expect(
      getIds(filterPresetEntries(presetEntries, "cn-gateway.example.com", t)),
    ).toEqual([]);
    expect(getIds(filterPresetEntries(presetEntries, "聚合", t))).toEqual([]);
  });

  it("支持 A-Z 排序、original 副本恢复原顺序，并且 getVisible 先 filter 再 sort", () => {
    const originalMode: PresetSortMode = "original";
    const nameAscMode: PresetSortMode = "nameAsc";

    const original = sortPresetEntries(presetEntries, originalMode, t);
    expect(original).not.toBe(presetEntries);
    expect(getIds(original)).toEqual(["gamma", "alpha", "beta", "delta"]);

    expect(getIds(sortPresetEntries(presetEntries, nameAscMode, t))).toEqual([
      "alpha",
      "beta",
      "delta",
      "gamma",
    ]);
    expect(getIds(presetEntries)).toEqual(["gamma", "alpha", "beta", "delta"]);

    expect(
      getIds(
        getVisiblePresetEntries(presetEntries, {
          query: "a",
          sortMode: nameAscMode,
          t,
        }),
      ),
    ).toEqual(["alpha", "beta", "delta", "gamma"]);
  });
});

describe("ProviderPresetSelector", () => {
  it("渲染常驻的「自定义配置」按钮与各分类标签页，默认激活第一个可用分类", () => {
    renderSelector();

    // 自定义配置是独立的常驻按钮，不是分类标签页的一部分
    expect(
      screen.getByRole("button", { name: "providerPreset.custom" }),
    ).toBeInTheDocument();
    expect(getTabTexts()).toEqual(["官方", "国产官方", "聚合服务", "第三方"]);

    // 默认激活优先级最高的可用分类（official），只展示该分类下的预设
    expect(getPresetButtonTexts()).toEqual(["preset.alpha"]);
  });

  it("点击分类标签后切换展示对应分类的预设，其余分类的预设不再展示", async () => {
    const user = userEvent.setup();
    renderSelector();

    await user.click(screen.getByRole("tab", { name: "国产官方" }));
    expect(getPresetButtonTexts()).toEqual(["Beta Gateway"]);

    await user.click(screen.getByRole("tab", { name: "聚合服务" }));
    expect(getPresetButtonTexts()).toEqual(["preset.gamma"]);
  });

  it("点击「自定义配置」按钮立即选中 custom，且不影响当前分类标签页下预设的展示", async () => {
    const user = userEvent.setup();
    const onPresetChange = vi.fn();
    renderSelector({ onPresetChange });

    await user.click(
      screen.getByRole("button", { name: "providerPreset.custom" }),
    );

    expect(onPresetChange).toHaveBeenCalledWith("custom");
    // 自定义配置只是标记选中状态，不是分类标签页，预设网格（当前分类 official）保持可见
    expect(
      screen.getByRole("button", { name: "preset.alpha" }),
    ).toBeInTheDocument();
  });

  it("点击排序按钮后对当前分类内的 preset A-Z 排序，再点恢复原顺序", async () => {
    const user = userEvent.setup();
    renderSelector({ entries: sameCategoryEntries });

    expect(getPresetButtonTexts()).toEqual([
      "preset.gamma",
      "preset.alpha",
      "Beta Gateway",
    ]);

    await user.click(getSortButton());
    expect(getPresetButtonTexts()).toEqual([
      "Beta Gateway",
      "preset.alpha",
      "preset.gamma",
    ]);

    await user.click(getSortButton());
    expect(getPresetButtonTexts()).toEqual([
      "preset.gamma",
      "preset.alpha",
      "Beta Gateway",
    ]);
  });

  it("搜索跨分类展平匹配的 preset，自定义配置标签始终保留", async () => {
    const user = userEvent.setup();
    renderSelector();

    await user.click(getSearchButton());
    await user.type(getSearchInput(), "gateway");

    expect(
      screen.getByRole("button", { name: "providerPreset.custom" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Beta Gateway" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "preset.gamma" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "preset.alpha" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delta Mirror" }),
    ).not.toBeInTheDocument();
  });

  it("搜索无普通 preset 结果时保留自定义配置标签并显示空状态", async () => {
    const user = userEvent.setup();
    renderSelector();

    await user.click(getSearchButton());
    await user.type(getSearchInput(), "not-found");

    expect(
      screen.getByRole("button", { name: "providerPreset.custom" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "preset.gamma" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "preset.alpha" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Beta Gateway" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delta Mirror" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(
        /providerPreset\.(empty|noResults)|没有匹配|无结果|no matching presets/i,
      ),
    ).toBeInTheDocument();
  });

  it("同一分类下所有预设按钮填满网格列宽(w-full)实现等宽对齐", () => {
    renderSelector({ entries: sameCategoryEntries });

    const presetButtons = screen.getAllByRole("button");
    const fullWidthButtons = presetButtons.filter((btn) =>
      btn.className.includes("w-full"),
    );

    // 3 个同分类预设 = 3 个等宽按钮(搜索/排序按钮为 size-8 不计入)
    expect(fullWidthButtons.length).toBeGreaterThanOrEqual(3);
  });

  it("preset.icon 存在时按钮内渲染图标元素(img/svg)", () => {
    const entriesWithIcon = [
      {
        id: "with-icon",
        preset: {
          name: "With Icon",
          websiteUrl: "https://icon.example.com",
          settingsConfig: {},
          category: "official" as ProviderCategory,
          icon: "claude-api",
          iconColor: "#D4915D",
        },
      },
    ];

    renderSelector({ entries: entriesWithIcon });

    const button = screen.getByRole("button", { name: /with icon/i });
    const icon = button.querySelector('[data-testid="provider-icon"]');
    expect(icon).not.toBeNull();
    expect(icon?.getAttribute("data-icon")).toBe("claude-api");
    expect(icon?.getAttribute("data-color")).toBe("#D4915D");
  });

  it("preset 无 icon 且无 theme.icon 时,按钮内仍渲染占位元素保持文字对齐", () => {
    const entriesWithoutIcon = [
      {
        id: "no-icon",
        preset: {
          name: "No Icon",
          websiteUrl: "https://noicon.example.com",
          settingsConfig: {},
          category: "official" as ProviderCategory,
        },
      },
    ];

    renderSelector({ entries: entriesWithoutIcon });

    const button = screen.getByRole("button", { name: /no icon/i });
    // 占位 span(16x16)应该存在,保证文字位置与有图标的按钮对齐
    const placeholder = button.querySelector("span[aria-hidden]");
    expect(placeholder).not.toBeNull();
  });

  it("点击放大镜 inline 切换搜索输入框可见性,ESC 收起并清空", async () => {
    const user = userEvent.setup();
    renderSelector();

    // 初始没有搜索输入框
    expect(
      screen.queryByRole("textbox", {
        name: /providerPreset\.(searchInput|searchPlaceholder)|搜索预设|search/i,
      }),
    ).not.toBeInTheDocument();

    // 点击放大镜展开输入框
    await user.click(getSearchButton());
    const input = getSearchInput();
    expect(input).toBeInTheDocument();

    // 输入关键字过滤（跨分类展平，能匹配到 cn_official 分类下的 Beta Gateway）
    await user.type(input, "gateway");
    expect(
      screen.getByRole("button", { name: "Beta Gateway" }),
    ).toBeInTheDocument();

    // ESC 收起输入框并清空
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("textbox", {
        name: /providerPreset\.(searchInput|searchPlaceholder)|搜索预设|search/i,
      }),
    ).not.toBeInTheDocument();
    // 收起后恢复到收起前的分类标签页（默认 official），preset.alpha 重新可见
    expect(
      screen.getByRole("button", { name: "preset.alpha" }),
    ).toBeInTheDocument();
  });

  it("按 Ctrl+F 快捷键打开搜索输入框", async () => {
    const user = userEvent.setup();
    renderSelector();

    // 初始没有搜索输入框
    expect(
      screen.queryByRole("textbox", {
        name: /providerPreset\.(searchInput|searchPlaceholder)|搜索预设|search/i,
      }),
    ).not.toBeInTheDocument();

    // 按 Ctrl+F 展开输入框
    await user.keyboard("{Control>}f{/Control}");
    expect(getSearchInput()).toBeInTheDocument();
  });

  it("搜索后点击预设按钮可选中预设且不清空搜索关键词", async () => {
    const user = userEvent.setup();
    const onPresetChange = vi.fn();
    renderSelector({ onPresetChange });

    await user.click(getSearchButton());
    await user.type(getSearchInput(), "gateway");

    await user.click(screen.getByRole("button", { name: "Beta Gateway" }));

    expect(onPresetChange).toHaveBeenCalledWith("beta");
    // 搜索框仍展开、关键词保留
    expect(getSearchInput()).toBeInTheDocument();
    expect(getSearchInput()).toHaveValue("gateway");
  });

  it("搜索已打开、焦点在别处时再次 Ctrl+F 把焦点移回搜索框且保留关键词", async () => {
    const user = userEvent.setup();
    renderSelector();

    await user.click(getSearchButton());
    await user.type(getSearchInput(), "gateway");

    // 选中 preset 后焦点离开搜索框（搜索框仍展开、关键词保留）
    await user.click(screen.getByRole("button", { name: "Beta Gateway" }));
    expect(getSearchInput()).not.toHaveFocus();

    // 再次 Ctrl+F：setSearchOpen(true) 同值不重渲染、autoFocus 不重触发，
    // 需靠快捷键命中时的命令式聚焦把焦点移回搜索框，且不清空关键词
    await user.keyboard("{Control>}f{/Control}");
    await waitFor(() => expect(getSearchInput()).toHaveFocus());
    expect(getSearchInput()).toHaveValue("gateway");
  });

  it("点击组件外区域自动收起并清空", async () => {
    const user = userEvent.setup();
    const Wrapper = () => {
      const form = useForm();
      return (
        <Form {...form}>
          <ProviderPresetSelector
            selectedPresetId="custom"
            presetEntries={presetEntries}
            presetCategoryLabels={presetCategoryLabels}
            onPresetChange={vi.fn()}
          />
          <div data-testid="outside">Outside</div>
        </Form>
      );
    };
    render(<Wrapper />);

    await user.click(getSearchButton());
    await user.type(getSearchInput(), "gateway");
    expect(getSearchInput()).toBeInTheDocument();

    // 点击组件外的元素应收起搜索框
    await user.click(screen.getByTestId("outside"));

    expect(
      screen.queryByRole("textbox", {
        name: /providerPreset\.(searchInput|searchPlaceholder)|搜索预设|search/i,
      }),
    ).not.toBeInTheDocument();
    // 收起后清空 query，恢复到收起前的分类标签页（默认 official）
    expect(
      screen.getByRole("button", { name: "preset.alpha" }),
    ).toBeInTheDocument();
  });
});
