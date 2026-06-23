import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClaudeDesktopRouteToggle } from "@/components/proxy/ClaudeDesktopRouteToggle";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import type { ProxyServerInfo } from "@/types/proxy";

vi.mock("@/hooks/useProxyStatus", () => ({
  useProxyStatus: vi.fn(),
}));

const useProxyStatusMock = vi.mocked(useProxyStatus);

// mutation mock 返回 Promise，与真实 mutateAsync 契约一致（裸 vi.fn() 返回 undefined 会掩盖 await 语义）。
// start_proxy_server 解析为 ProxyServerInfo，停止类命令解析为 void，类型与 hook 返回值对齐。
const startProxyServerMock = vi.fn(() => Promise.resolve({} as ProxyServerInfo));
const stopProxyServerMock = vi.fn(() => Promise.resolve());
const stopWithRestoreMock = vi.fn(() => Promise.resolve());

interface MockProxyStatusOptions {
  isRunning?: boolean;
  otherTakeoverActive?: boolean;
  isStarting?: boolean;
  isStoppingServer?: boolean;
  isStopping?: boolean;
}

/**
 * 构造一份完整的 useProxyStatus 返回值。返回类型显式标注为 ReturnType<typeof useProxyStatus>，
 * 这样赋值时 TS 会逐字段把关，缺字段（例如 ProxyTakeoverStatus 必填的 hermes）会直接报错，
 * 不再需要 `as unknown as ...` 双重断言掩盖。
 */
function mockProxyStatus({
  isRunning = true,
  otherTakeoverActive = true,
  isStarting = false,
  isStoppingServer = false,
  isStopping = false,
}: MockProxyStatusOptions = {}): ReturnType<typeof useProxyStatus> {
  const result: ReturnType<typeof useProxyStatus> = {
    status: {
      running: isRunning,
      address: "127.0.0.1",
      port: 15721,
      active_connections: 0,
      total_requests: 0,
      success_requests: 0,
      failed_requests: 0,
      success_rate: 0,
      uptime_seconds: 0,
      current_provider: null,
      current_provider_id: null,
      last_request_at: null,
      last_error: null,
      failover_count: 0,
    },
    isLoading: false,
    isRunning,
    // ProxyTakeoverStatus 必填字段需全部给出：claude/codex/gemini/opencode/openclaw/hermes
    takeoverStatus: {
      claude: otherTakeoverActive,
      codex: false,
      gemini: false,
      opencode: false,
      openclaw: false,
      hermes: false,
    },
    isTakeoverActive: otherTakeoverActive,
    startProxyServer: startProxyServerMock,
    stopProxyServer: stopProxyServerMock,
    stopWithRestore: stopWithRestoreMock,
    setTakeoverForApp: vi.fn(),
    switchProxyProvider: vi.fn(),
    checkRunning: vi.fn(),
    checkTakeoverActive: vi.fn(),
    isStarting,
    isStoppingServer,
    isStopping,
    isPending: isStarting || isStoppingServer || isStopping,
  };
  useProxyStatusMock.mockReturnValue(result);
  return result;
}

describe("ClaudeDesktopRouteToggle", () => {
  beforeEach(() => {
    startProxyServerMock.mockReset();
    stopProxyServerMock.mockReset();
    stopWithRestoreMock.mockReset();
    // 默认恢复为返回 resolved Promise 的实现
    startProxyServerMock.mockReturnValue(Promise.resolve({} as ProxyServerInfo));
    stopProxyServerMock.mockReturnValue(Promise.resolve());
    stopWithRestoreMock.mockReturnValue(Promise.resolve());
    mockProxyStatus();
  });

  it("asks for confirmation before stopping while other apps use takeover", async () => {
    render(<ClaudeDesktopRouteToggle />);

    const toggle = screen.getByRole("switch");
    expect(toggle).toBeChecked();

    fireEvent.click(toggle);

    expect(stopProxyServerMock).not.toHaveBeenCalled();
    expect(await screen.findByText("确认停止本地路由？")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "保持开启" }));
    expect(stopProxyServerMock).not.toHaveBeenCalled();
    expect(stopWithRestoreMock).not.toHaveBeenCalled();
    expect(screen.queryByText("确认停止本地路由？")).not.toBeInTheDocument();

    fireEvent.click(toggle);
    fireEvent.click(await screen.findByRole("button", { name: "仍然停止" }));

    await waitFor(() => {
      expect(stopWithRestoreMock).toHaveBeenCalledTimes(1);
    });
    expect(stopProxyServerMock).not.toHaveBeenCalled();
  });

  it("stops immediately when no other takeover is active", async () => {
    mockProxyStatus({ otherTakeoverActive: false });
    render(<ClaudeDesktopRouteToggle />);

    fireEvent.click(screen.getByRole("switch"));

    await waitFor(() => {
      expect(stopProxyServerMock).toHaveBeenCalledTimes(1);
    });
    expect(stopWithRestoreMock).not.toHaveBeenCalled();
    expect(screen.queryByText("确认停止本地路由？")).not.toBeInTheDocument();
  });

  it("starts the proxy when toggled on from a stopped state", async () => {
    mockProxyStatus({ isRunning: false, otherTakeoverActive: false });
    render(<ClaudeDesktopRouteToggle />);

    const toggle = screen.getByRole("switch");
    expect(toggle).not.toBeChecked();

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(startProxyServerMock).toHaveBeenCalledTimes(1);
    });
    expect(stopProxyServerMock).not.toHaveBeenCalled();
    expect(stopWithRestoreMock).not.toHaveBeenCalled();
  });

  it("disables the switch and ignores clicks while a stop is in progress", async () => {
    mockProxyStatus({ isStopping: true });
    render(<ClaudeDesktopRouteToggle />);

    const toggle = screen.getByRole("switch");
    expect(toggle).toBeDisabled();

    fireEvent.click(toggle);
    expect(startProxyServerMock).not.toHaveBeenCalled();
    expect(stopProxyServerMock).not.toHaveBeenCalled();
    expect(stopWithRestoreMock).not.toHaveBeenCalled();
  });

  it("rolls back gracefully when starting the proxy rejects without crashing the UI", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    startProxyServerMock.mockReturnValueOnce(
      Promise.reject<ProxyServerInfo>(new Error("boom")),
    );

    mockProxyStatus({ isRunning: false, otherTakeoverActive: false });
    render(<ClaudeDesktopRouteToggle />);

    // 组件内有 try/catch + console.error，rejected 不应抛出未捕获异常导致 UI 崩溃
    expect(() => fireEvent.click(screen.getByRole("switch"))).not.toThrow();

    await waitFor(() => {
      expect(startProxyServerMock).toHaveBeenCalledTimes(1);
    });
    // UI 仍可交互：开关节点仍在文档中
    expect(screen.getByRole("switch")).toBeInTheDocument();

    errorSpy.mockRestore();
  });

  it("closes the confirmation dialog after confirming the stop", async () => {
    render(<ClaudeDesktopRouteToggle />);

    fireEvent.click(screen.getByRole("switch"));
    expect(await screen.findByText("确认停止本地路由？")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "仍然停止" }));

    await waitFor(() => {
      expect(stopWithRestoreMock).toHaveBeenCalledTimes(1);
    });
    // 确认后弹窗应消失（之前仅测过取消后关闭）
    expect(screen.queryByText("确认停止本地路由？")).not.toBeInTheDocument();
  });
});
