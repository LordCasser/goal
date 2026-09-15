/**
 * 候选回答内联标记的解析（openspec planner-workspace「候选回答由模型内联
 * 给出」）。模型在回复正文里写 `<next_steps>A | B | C</next_steps>`，竖线
 * 分隔候选项；标记属于**对话内容**，快捷回复按钮属于**界面控件**——本纯
 * 函数把两者解耦：正文剥离标记，候选项交给界面渲染并绑定 ⌘1…⌘9。
 */

export type ParsedNextSteps = {
  /** 剥离标记后的正文；无标记时为原文原样返回。 */
  body: string;
  /** 候选项（已 trim，空项跳过，按出现顺序）。 */
  options: string[];
};

// 非贪婪匹配完整的一对标记；标记可出现多处（/g），未闭合的半个标记不匹配。
const NEXT_STEPS_BLOCK = /<next_steps>([\s\S]*?)<\/next_steps>/g;

/**
 * 解析规则：
 * - 没有完整标记（含被截断的未闭合标记）→ 原文返回，不产出候选项；
 * - 标记出现多处时全部剥离，候选项按出现顺序合并；
 * - 竖线前后空白 trim；拆分出的空项跳过，不渲染空按钮（含竖线的候选项
 *   该格式表达不了，按约定由模型避免）；
 * - 有标记时正文为剥离后的剩余文本，仅做首尾 trim。
 */
export function parseNextSteps(text: string): ParsedNextSteps {
  const options: string[] = [];
  let found = false;
  const stripped = text.replace(NEXT_STEPS_BLOCK, (_match, inner: string) => {
    found = true;
    for (const part of inner.split("|")) {
      const option = part.trim();
      if (option !== "") options.push(option);
    }
    return "";
  });
  return found ? { body: stripped.trim(), options } : { body: text, options: [] };
}
