import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCodexCommonConfig } from "@/components/providers/forms/hooks/useCodexCommonConfig";
import { useGeminiCommonConfig } from "@/components/providers/forms/hooks/useGeminiCommonConfig";

const getCommonConfigSnippetMock = vi.fn();
const setCommonConfigSnippetMock = vi.fn();
const extractCommonConfigSnippetMock = vi.fn();
const updateTomlCommonConfigSnippetMock = vi.fn();

vi.mock("@/lib/api", () => ({
  configApi: {
    getCommonConfigSnippet: (...args: unknown[]) =>
      getCommonConfigSnippetMock(...args),
    setCommonConfigSnippet: (...args: unknown[]) =>
      setCommonConfigSnippetMock(...args),
    extractCommonConfigSnippet: (...args: unknown[]) =>
      extractCommonConfigSnippetMock(...args),
    updateTomlCommonConfigSnippet: (...args: unknown[]) =>
      updateTomlCommonConfigSnippetMock(...args),
  },
}));

describe("common config snippet saving", () => {
  beforeEach(() => {
    window.localStorage.clear();
    getCommonConfigSnippetMock.mockReset().mockResolvedValue("");
    setCommonConfigSnippetMock.mockReset().mockResolvedValue(undefined);
    extractCommonConfigSnippetMock.mockReset().mockResolvedValue("");
    updateTomlCommonConfigSnippetMock
      .mockReset()
      .mockImplementation(async (configToml: string) => configToml);
  });

  it("routes Codex Desktop common-config operations to its own target", async () => {
    getCommonConfigSnippetMock.mockResolvedValue(
      "[tui]\nnotifications = true\n",
    );
    extractCommonConfigSnippetMock.mockResolvedValue(
      '[history]\npersistence = "save-all"\n',
    );
    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCodexCommonConfig({
        appType: "codex-desktop",
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
        initialData: { settingsConfig: { config: 'model = "gpt-5"' } },
        initialEnabled: false,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(getCommonConfigSnippetMock).toHaveBeenCalledWith("codex-desktop");
    expect(result.current.useCommonConfig).toBe(false);

    await act(async () => {
      await result.current.handleCommonConfigSnippetChange(
        'base_url = "https://example.com"',
      );
      await result.current.handleExtract();
    });

    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex-desktop",
      'base_url = "https://example.com"',
    );
    expect(extractCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex-desktop",
      { settingsConfig: JSON.stringify({ config: 'model = "gpt-5"' }) },
    );
    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex-desktop",
      '[history]\npersistence = "save-all"\n',
    );
    expect(onConfigChange).not.toHaveBeenCalled();
  });

  it("invalidates an in-flight merge when the Codex target changes", async () => {
    getCommonConfigSnippetMock.mockImplementation(async (appType: string) =>
      appType === "codex"
        ? "[tui]\nnotifications = true\n"
        : "[desktop]\nenabled = true\n",
    );
    const initialData = { settingsConfig: { config: 'model = "gpt-5"' } };
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ appType }: { appType: "codex" | "codex-desktop" }) =>
        useCodexCommonConfig({
          appType,
          codexConfig: 'model = "gpt-5"',
          onConfigChange,
          initialData,
          initialEnabled: false,
        }),
      {
        initialProps: { appType: "codex" } as {
          appType: "codex" | "codex-desktop";
        },
      },
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let resolveMerge: ((value: string) => void) | undefined;
    updateTomlCommonConfigSnippetMock.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          resolveMerge = resolve;
        }),
    );

    let mergePending: Promise<void> = Promise.resolve();
    act(() => {
      mergePending = result.current.handleCommonConfigToggle(true);
    });
    rerender({ appType: "codex-desktop" });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    await waitFor(() =>
      expect(result.current.commonConfigSnippet).toContain("[desktop]"),
    );

    await act(async () => {
      resolveMerge?.('model = "gpt-5"\n\n[tui]\nnotifications = true\n');
      await mergePending;
    });

    expect(result.current.useCommonConfig).toBe(false);
    expect(result.current.commonConfigSnippet).toContain("[desktop]");
    expect(onConfigChange).not.toHaveBeenCalled();
  });

  it("does not let a stale extraction close the new target loading state", async () => {
    let resolveCliExtract: ((value: string) => void) | undefined;
    let resolveDesktopExtract: ((value: string) => void) | undefined;
    extractCommonConfigSnippetMock
      .mockImplementationOnce(
        () =>
          new Promise<string>((resolve) => {
            resolveCliExtract = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise<string>((resolve) => {
            resolveDesktopExtract = resolve;
          }),
      );
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ appType }: { appType: "codex" | "codex-desktop" }) =>
        useCodexCommonConfig({
          appType,
          codexConfig: 'model = "gpt-5"',
          onConfigChange,
          initialData: { settingsConfig: { config: 'model = "gpt-5"' } },
          initialEnabled: false,
        }),
      {
        initialProps: { appType: "codex" } as {
          appType: "codex" | "codex-desktop";
        },
      },
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let cliExtractPending: Promise<void> = Promise.resolve();
    act(() => {
      cliExtractPending = result.current.handleExtract();
    });
    await waitFor(() => expect(result.current.isExtracting).toBe(true));

    rerender({ appType: "codex-desktop" });
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let desktopExtractPending: Promise<void> = Promise.resolve();
    act(() => {
      desktopExtractPending = result.current.handleExtract();
    });
    await waitFor(() => expect(result.current.isExtracting).toBe(true));

    await act(async () => {
      resolveCliExtract?.("[cli]\nvalue = 1\n");
      await cliExtractPending;
    });
    expect(result.current.isExtracting).toBe(true);

    await act(async () => {
      resolveDesktopExtract?.("[desktop]\nvalue = 2\n");
      await desktopExtractPending;
    });
    expect(result.current.isExtracting).toBe(false);
    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex-desktop",
      "[desktop]\nvalue = 2\n",
    );
    expect(setCommonConfigSnippetMock).not.toHaveBeenCalledWith(
      "codex",
      "[cli]\nvalue = 1\n",
    );
  });

  it("keeps CLI and Desktop snippets isolated across target changes", async () => {
    getCommonConfigSnippetMock.mockImplementation(async (appType: string) =>
      appType === "codex" ? "[cli]\nvalue = 1\n" : "[desktop]\nvalue = 2\n",
    );
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ appType }: { appType: "codex" | "codex-desktop" }) =>
        useCodexCommonConfig({
          appType,
          codexConfig: 'model = "gpt-5"',
          onConfigChange,
          initialData: { settingsConfig: { config: 'model = "gpt-5"' } },
          initialEnabled: false,
        }),
      {
        initialProps: { appType: "codex" } as {
          appType: "codex" | "codex-desktop";
        },
      },
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.commonConfigSnippet).toContain("[cli]");

    rerender({ appType: "codex-desktop" });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    await waitFor(() =>
      expect(result.current.commonConfigSnippet).toContain("[desktop]"),
    );

    await act(async () => {
      await result.current.handleCommonConfigSnippetChange(
        "[desktop]\nvalue = 3\n",
      );
    });
    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex-desktop",
      "[desktop]\nvalue = 3\n",
    );

    rerender({ appType: "codex" });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    await waitFor(() =>
      expect(result.current.commonConfigSnippet).toContain("[cli]"),
    );
    expect(result.current.commonConfigSnippet).not.toContain("[desktop]");
    expect(getCommonConfigSnippetMock).toHaveBeenCalledWith("codex");
    expect(getCommonConfigSnippetMock).toHaveBeenCalledWith("codex-desktop");
  });

  it("migrates the legacy localStorage snippet only for Codex CLI", async () => {
    const legacySnippet = "[tui]\nnotifications = true\n";
    window.localStorage.setItem(
      "cc-switch:codex-common-config-snippet",
      legacySnippet,
    );
    const onConfigChange = vi.fn();
    const desktop = renderHook(() =>
      useCodexCommonConfig({
        appType: "codex-desktop",
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
      }),
    );

    await waitFor(() => expect(desktop.result.current.isLoading).toBe(false));
    expect(setCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(
      window.localStorage.getItem("cc-switch:codex-common-config-snippet"),
    ).toBe(legacySnippet);
    desktop.unmount();

    const cli = renderHook(() =>
      useCodexCommonConfig({
        appType: "codex",
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
      }),
    );

    await waitFor(() => expect(cli.result.current.isLoading).toBe(false));
    expect(setCommonConfigSnippetMock).toHaveBeenCalledWith(
      "codex",
      legacySnippet,
    );
    expect(
      window.localStorage.getItem("cc-switch:codex-common-config-snippet"),
    ).toBeNull();
  });

  it("does not persist an invalid Codex common config snippet", async () => {
    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCodexCommonConfig({
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let saved = true;
    await act(async () => {
      saved = await result.current.handleCommonConfigSnippetChange(
        "base_url = https://bad.example/v1",
      );
    });

    expect(saved).toBe(false);
    expect(setCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(onConfigChange).not.toHaveBeenCalled();
    expect(result.current.commonConfigError).toContain("invalid value");
  });

  it("discards stale toggle results when a newer toggle finishes first", async () => {
    getCommonConfigSnippetMock.mockResolvedValue(
      "[tui]\nnotifications = true\n",
    );

    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCodexCommonConfig({
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
        initialData: { settingsConfig: { config: 'model = "gpt-5"' } },
        initialEnabled: false,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    await waitFor(() => expect(result.current.useCommonConfig).toBe(false));

    // 第一次调用（勾选 on 的 merge）挂起，第二次（取消勾选的剥离）立即返回：
    // 模拟后端乱序完成
    let resolveMerge: ((value: string) => void) | undefined;
    updateTomlCommonConfigSnippetMock
      .mockImplementationOnce(
        () =>
          new Promise<string>((resolve) => {
            resolveMerge = resolve;
          }),
      )
      .mockImplementationOnce(async (configToml: string) => configToml);

    await act(async () => {
      const mergePending = result.current.handleCommonConfigToggle(true);
      const removeDone = result.current.handleCommonConfigToggle(false);
      await removeDone;
      // on 的合并结果此时才姗姗来迟——必须被序号守卫丢弃
      resolveMerge?.('model = "gpt-5"\n\n[tui]\nnotifications = true\n');
      await mergePending;
    });

    // 用户最后一次操作是 off：过期的 on 结果不得翻转开关或改写配置
    expect(result.current.useCommonConfig).toBe(false);
    const lastConfig = onConfigChange.mock.calls.at(-1)?.[0] as string;
    expect(lastConfig).not.toContain("[tui]");
  });

  it("discards async merge results when the user edited the config while in flight", async () => {
    getCommonConfigSnippetMock.mockResolvedValue(
      "[tui]\nnotifications = true\n",
    );

    const initialData = { settingsConfig: { config: 'model = "gpt-5"' } };
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ config }: { config: string }) =>
        useCodexCommonConfig({
          codexConfig: config,
          onConfigChange,
          initialData,
          initialEnabled: false,
        }),
      { initialProps: { config: 'model = "gpt-5"' } },
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    await waitFor(() => expect(result.current.useCommonConfig).toBe(false));

    let resolveMerge: ((value: string) => void) | undefined;
    updateTomlCommonConfigSnippetMock.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          resolveMerge = resolve;
        }),
    );

    let togglePending: Promise<void> = Promise.resolve();
    act(() => {
      togglePending = result.current.handleCommonConfigToggle(true);
    });

    // merge 在飞期间，用户在编辑器里手动改了 config（不经过 hook，
    // 序号不变，只有 codexConfig prop 变化）
    rerender({ config: 'model = "gpt-6-user-edit"' });

    await act(async () => {
      resolveMerge?.('model = "gpt-5"\n\n[tui]\nnotifications = true\n');
      await togglePending;
    });

    // 基于陈旧基线的合并结果必须被丢弃，不得覆盖用户的手动编辑
    expect(onConfigChange).not.toHaveBeenCalled();
    expect(result.current.useCommonConfig).toBe(false);
  });

  it("does not persist an invalid Gemini common config snippet", async () => {
    const onEnvChange = vi.fn();
    const { result } = renderHook(() =>
      useGeminiCommonConfig({
        envValue: "",
        onEnvChange,
        envStringToObj: () => ({}),
        envObjToString: () => "",
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    let saved = false;
    act(() => {
      saved = result.current.handleCommonConfigSnippetChange(
        JSON.stringify({ GEMINI_MODEL: 123 }),
      );
    });

    expect(saved).toBe(false);
    expect(setCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(onEnvChange).not.toHaveBeenCalled();
    expect(result.current.commonConfigError).toBe(
      "geminiConfig.commonConfigInvalidValues",
    );
  });
});
