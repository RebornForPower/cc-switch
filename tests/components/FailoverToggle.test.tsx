import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FailoverToggle } from "@/components/proxy/FailoverToggle";

const mocks = vi.hoisted(() => ({
  mutate: vi.fn(),
}));

vi.mock("@/lib/query/failover", () => ({
  useAutoFailoverEnabled: () => ({ data: false, isLoading: false }),
  useSetAutoFailoverEnabled: () => ({
    mutate: mocks.mutate,
    isPending: false,
  }),
}));

vi.mock("@/hooks/useProxyStatus", () => ({
  useProxyStatus: () => ({
    isRunning: false,
    takeoverStatus: {
      claude: false,
      codex: false,
      gemini: false,
      grokbuild: false,
    },
  }),
}));

describe("FailoverToggle", () => {
  it("asks Codex Desktop users to start local routing", () => {
    render(<FailoverToggle activeApp="codex-desktop" />);

    expect(
      screen.getByTitle("请先开启 Codex Desktop 本地路由，再启用故障转移"),
    ).toBeInTheDocument();
  });

  it("keeps the takeover wording for CLI applications", () => {
    render(<FailoverToggle activeApp="codex" />);

    expect(
      screen.getByTitle("请先接管 Codex CLI，再启用故障转移"),
    ).toBeInTheDocument();
  });
});
