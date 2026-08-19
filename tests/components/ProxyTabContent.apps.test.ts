import { describe, expect, it } from "vitest";
import { FAILOVER_APPS } from "@/components/settings/ProxyTabContent";

describe("ProxyTabContent failover apps", () => {
  it("exposes every application with a failover data plane", () => {
    expect(FAILOVER_APPS.map(({ id }) => id)).toEqual([
      "claude",
      "codex",
      "codex-desktop",
      "gemini",
      "grokbuild",
    ]);
  });
});
