/**
 * 提示注册表纯逻辑：触发条件成立才入选，已关闭的 id 永远不出现
 * （onboarding-guidance「就地提示」2.1/2.2）。
 */
import { describe, expect, it } from "vitest";
import { HINTS, hintById, visibleHints, type HintContext } from "./hints";

const NO_FACTS: HintContext = {};

describe("hint registry", () => {
  it("exposes stable ids with copy", () => {
    for (const hint of HINTS) {
      expect(hint.id).toMatch(/^[a-z0-9-]+$/);
      expect(hint.titleKey.length).toBeGreaterThan(0);
      expect(hint.bodyKey.length).toBeGreaterThan(0);
    }
  });

  it("shows the later explainer only while Later is empty", () => {
    const laterHint = hintById("later-explainer")!;
    expect(laterHint.when(NO_FACTS)).toBe(true);
    expect(laterHint.when({ laterCount: 0 })).toBe(true);
    expect(laterHint.when({ laterCount: 2 })).toBe(false);
  });

  it("shows the duration explainer only after a durationless session", () => {
    const hint = hintById("session-duration-explainer")!;
    expect(hint.when(NO_FACTS)).toBe(false);
    expect(hint.when({ showedDurationlessSession: true })).toBe(true);
  });

  it("shows the cross-link explainer near the end of the guide without links", () => {
    const hint = hintById("cross-link-explainer")!;
    expect(hint.when({ guideNearComplete: true })).toBe(true);
    expect(hint.when({ guideNearComplete: true, linkedItemCount: 1 })).toBe(false);
    expect(hint.when({ guideNearComplete: false })).toBe(false);
  });

  it("filters dismissed ids before trigger conditions", () => {
    const all = visibleHints({ laterCount: 0 }, []);
    expect(all.map((h) => h.id)).toContain("later-explainer");
    const filtered = visibleHints({ laterCount: 0 }, ["later-explainer"]);
    expect(filtered.map((h) => h.id)).not.toContain("later-explainer");
    // Dismissed stays hidden even when its trigger would newly hold.
    expect(
      visibleHints(
        { laterCount: 2, showedDurationlessSession: true },
        ["session-duration-explainer"],
      ),
    ).toEqual([]);
  });
});
