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
    getCommonConfigSnippetMock.mockResolvedValue("");
    setCommonConfigSnippetMock.mockResolvedValue(undefined);
    extractCommonConfigSnippetMock.mockResolvedValue("");
    updateTomlCommonConfigSnippetMock.mockImplementation(
      async (configToml: string) => configToml,
    );
  });

  it("keeps the Codex Desktop target out of the CLI common-config store", async () => {
    const onConfigChange = vi.fn();
    const { result } = renderHook(() =>
      useCodexCommonConfig({
        codexConfig: 'model = "gpt-5"',
        onConfigChange,
        enabled: false,
      }),
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(getCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(result.current.useCommonConfig).toBe(false);
    expect(result.current.commonConfigSnippet).toBe("");

    await act(async () => {
      await result.current.handleCommonConfigSnippetChange(
        'base_url = "https://example.com"',
      );
      await result.current.handleCommonConfigToggle(true);
      await result.current.handleExtract();
    });

    expect(setCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(extractCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(onConfigChange).not.toHaveBeenCalled();
  });

  it("invalidates an in-flight CLI merge when the target becomes Desktop", async () => {
    getCommonConfigSnippetMock.mockResolvedValue(
      "[tui]\nnotifications = true\n",
    );
    const initialData = { settingsConfig: { config: 'model = "gpt-5"' } };
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useCodexCommonConfig({
          codexConfig: 'model = "gpt-5"',
          onConfigChange,
          initialData,
          initialEnabled: false,
          enabled,
        }),
      { initialProps: { enabled: true } },
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
    rerender({ enabled: false });

    await act(async () => {
      resolveMerge?.('model = "gpt-5"\n\n[tui]\nnotifications = true\n');
      await mergePending;
    });

    expect(result.current.useCommonConfig).toBe(false);
    expect(result.current.commonConfigSnippet).toBe("");
    expect(onConfigChange).not.toHaveBeenCalled();
  });

  it("reloads the CLI snippet before initializing after Desktop is re-enabled", async () => {
    let resolveLoad: ((value: string) => void) | undefined;
    getCommonConfigSnippetMock.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          resolveLoad = resolve;
        }),
    );
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useCodexCommonConfig({
          codexConfig: 'model = "gpt-5"',
          onConfigChange,
          enabled,
        }),
      { initialProps: { enabled: false } },
    );

    expect(result.current.isLoading).toBe(false);
    rerender({ enabled: true });

    await waitFor(() => expect(result.current.isLoading).toBe(true));
    expect(updateTomlCommonConfigSnippetMock).not.toHaveBeenCalled();

    await act(async () => {
      resolveLoad?.("[tui]\nnotifications = true\n");
    });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    await waitFor(() =>
      expect(updateTomlCommonConfigSnippetMock).toHaveBeenCalledWith(
        'model = "gpt-5"',
        "[tui]\nnotifications = true\n",
        true,
      ),
    );
  });

  it("does not reuse a stale CLI snippet when the reloaded store is empty", async () => {
    getCommonConfigSnippetMock
      .mockResolvedValueOnce("[tui]\nnotifications = true\n")
      .mockResolvedValueOnce("");
    const onConfigChange = vi.fn();
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useCodexCommonConfig({
          codexConfig: 'model = "gpt-5"',
          onConfigChange,
          initialData: { settingsConfig: { config: 'model = "gpt-5"' } },
          initialEnabled: false,
          enabled,
        }),
      { initialProps: { enabled: true } },
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.commonConfigSnippet).toContain("[tui]");

    rerender({ enabled: false });
    rerender({ enabled: true });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.commonConfigSnippet).not.toContain("[tui]");
    expect(result.current.useCommonConfig).toBe(false);
    expect(updateTomlCommonConfigSnippetMock).not.toHaveBeenCalled();
    expect(onConfigChange).not.toHaveBeenCalled();
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
