/**
 * next_steps 内联标记的解析契约（openspec planner-workspace「候选回答由
 * 模型内联给出」）：剥离标记、竖线拆分、空项跳过、无标记原文返回。
 */
import { describe, expect, it } from "vitest";

import { parseNextSteps } from "./nextSteps";

describe("parseNextSteps", () => {
  it("returns the original text untouched when no marker is present", () => {
    const text = "没有候选的普通回复。";
    expect(parseNextSteps(text)).toEqual({ body: text, options: [] });
  });

  it("splits multiple options on pipes and strips the marker from the body", () => {
    const parsed = parseNextSteps(
      "我们可以继续。<next_steps>先做周计划 | 先设目标 | 先看问题报告</next_steps>",
    );
    expect(parsed.options).toEqual(["先做周计划", "先设目标", "先看问题报告"]);
    expect(parsed.body).toBe("我们可以继续。");
  });

  it("parses a single option and leaves an empty body", () => {
    const parsed = parseNextSteps("<next_steps>Start planning</next_steps>");
    expect(parsed.options).toEqual(["Start planning"]);
    expect(parsed.body).toBe("");
  });

  it("skips empty items between and after pipes", () => {
    const parsed = parseNextSteps("<next_steps>A | | B |</next_steps>");
    expect(parsed.options).toEqual(["A", "B"]);
  });

  it("keeps text that sits outside the marker", () => {
    const parsed = parseNextSteps("前文。<next_steps>A | B</next_steps>后文。");
    expect(parsed.body).toBe("前文。后文。");
    expect(parsed.options).toEqual(["A", "B"]);
  });

  it("trims whitespace around the pipes", () => {
    const parsed = parseNextSteps("<next_steps>  先做周计划  |  先设目标  </next_steps>");
    expect(parsed.options).toEqual(["先做周计划", "先设目标"]);
  });

  it("treats an unclosed marker as plain text", () => {
    const text = "<next_steps>A | B";
    expect(parseNextSteps(text)).toEqual({ body: text, options: [] });
  });

  it("merges options from several marker blocks", () => {
    const parsed = parseNextSteps(
      "<next_steps>A</next_steps>中间文字<next_steps>B | C</next_steps>",
    );
    expect(parsed.options).toEqual(["A", "B", "C"]);
    expect(parsed.body).toBe("中间文字");
  });
});
