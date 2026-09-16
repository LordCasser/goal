/**
 * 时间线推导的纯函数测试（任务 7.9）。期望值与 Rust 侧对齐：
 * domain/calendar.rs 的 calculate_ends_on 测试用例（2026-09-14 系列、
 * 2026-12-01 跨年、2024-01-01 跨闰日）+ 手工核算的 2026-12-20 + 168 天
 * = 2027-06-06（6 个产品月 = 168 天，不是日历上的半年）。
 */
import { describe, expect, it } from "vitest";
import { deriveCustomTimeline, deriveTimeline } from "./timeline";

describe("deriveTimeline weeks", () => {
  it("maps product months to 4 / 12 / 24 weeks", () => {
    expect(deriveTimeline("2026-09-14", 1).weeks).toBe(4);
    expect(deriveTimeline("2026-09-14", 3).weeks).toBe(12);
    expect(deriveTimeline("2026-09-14", 6).weeks).toBe(24);
  });
});

describe("deriveTimeline review date (Rust calculate_ends_on parity)", () => {
  it("ends a 1-month cycle 28 days later", () => {
    const { nodes } = deriveTimeline("2026-09-14", 1);
    expect(nodes[2]).toMatchObject({ key: "review", date: "2026-10-12" });
  });

  it("ends a 3-month cycle 84 days later", () => {
    const { nodes } = deriveTimeline("2026-09-14", 3);
    expect(nodes[2]).toMatchObject({ key: "review", date: "2026-12-07" });
  });

  it("ends a 6-month cycle 168 days later", () => {
    const { nodes } = deriveTimeline("2026-09-14", 6);
    expect(nodes[2]).toMatchObject({ key: "review", date: "2027-03-01" });
  });

  it("crosses the year boundary (December start)", () => {
    expect(deriveTimeline("2026-12-01", 3).nodes[2].date).toBe("2027-02-23");
    // 6 个产品月 = 168 天：2026-12-20 + 168 天 = 2027-06-06（手工核算，
    // 与 calendar.rs 的“按天不按日历月”规则一致）。
    expect(deriveTimeline("2026-12-20", 6).nodes[2].date).toBe("2027-06-06");
  });

  it("crosses a leap day (2024-02-29)", () => {
    expect(deriveTimeline("2024-01-01", 3).nodes[2].date).toBe("2024-03-25");
  });
});

describe("deriveTimeline nodes", () => {
  it("keeps the set node on the start date", () => {
    const { nodes } = deriveTimeline("2026-09-14", 3);
    expect(nodes[0]).toMatchObject({ key: "set", date: "2026-09-14" });
  });

  it("places the progress check at the half-way point (rounded up)", () => {
    expect(deriveTimeline("2026-09-14", 3).nodes[1]).toMatchObject({
      key: "progress",
      date: "2026-10-26", // start + 42 天（84 的一半）
    });
    expect(deriveTimeline("2026-09-14", 1).nodes[1].date).toBe("2026-09-28"); // +14 天
  });

  it("emits labels in set → progress → review order", () => {
    const { nodes } = deriveTimeline("2026-09-14", 3);
    expect(nodes.map((n) => n.key)).toEqual(["set", "progress", "review"]);
    expect(nodes.every((n) => n.label.length > 0)).toBe(true);
  });
});

describe("deriveTimeline input defense", () => {
  it("rejects empty strings", () => {
    expect(() => deriveTimeline("", 3)).toThrow();
  });

  it("rejects malformed dates", () => {
    for (const bad of [
      "garbage",
      "2026-9-14", // 未补零
      "2026-13-01", // 月份越界
      "2026-02-30", // 滚动日期
      "2026-09-14T00:00:00", // 带时间
      " 2026-09-14", // 带空白
      "26-09-14", // 非四位年份
    ]) {
      expect(() => deriveTimeline(bad, 3), bad).toThrow();
    }
  });

  it("rejects durations outside the 1/3/6 set", () => {
    // 类型上不可传入，运行时仍防御（弹窗状态不会出现 2）。
    expect(() => deriveTimeline("2026-09-14", 2 as never)).toThrow();
    expect(() => deriveTimeline("2026-09-14", 0 as never)).toThrow();
  });
});

describe("deriveCustomTimeline", () => {
  it("uses date-only differences for non-week custom ranges", () => {
    expect(deriveCustomTimeline("2026-09-15", "2026-09-25", { kind: "repeat", every_days: 3 })).toMatchObject({
      days: 10,
      totalChecks: 3,
      checkDates: ["2026-09-18", "2026-09-21", "2026-09-24"],
      error: null,
    });
  });

  it("keeps a bounded preview while reporting all repeat checks", () => {
    const preview = deriveCustomTimeline("2026-01-01", "2026-12-31", { kind: "repeat", every_days: 1 });
    expect(preview.totalChecks).toBe(363);
    expect(preview.checkDates).toHaveLength(4);
    expect(preview.checkDates[0]).toBe("2026-01-02");
  });

  it("accepts a once check on the start date but excludes the end date", () => {
    expect(deriveCustomTimeline("2026-09-15", "2026-09-25", { kind: "once", date: "2026-09-15" }).error).toBeNull();
    expect(deriveCustomTimeline("2026-09-15", "2026-09-25", { kind: "once", date: "2026-09-25" }).error).toBe("invalid_progress_check");
  });

  it("classifies invalid ranges and intervals for localized UI errors", () => {
    expect(deriveCustomTimeline("2026-09-25", "2026-09-15", { kind: "repeat", every_days: 7 }).error).toBe("invalid_cycle_range");
    expect(deriveCustomTimeline("2026-09-15", "2026-09-25", { kind: "repeat", every_days: 0 }).error).toBe("invalid_progress_check");
  });
});
