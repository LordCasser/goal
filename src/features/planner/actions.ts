/**
 * planner 共用的动作执行壳：统一把后端 `{code, message}` 错误转成可展示
 * 文案（design.md §5.2「保存失败」：行级说明，不假装已保存），并提供一个
 * 绝对定位的短错误条，让列头/卡片里的小菜单也能就地反馈而不挤压布局。
 * 事件驱动的失效之外，这里在每次写成功后补充失效对应查询——后端事件
 * 是主通道，补一次失效让无事件环境（测试、dev）也保持一致。
 */
import { useCallback, useState } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { isAppError } from "../../lib/ipc";
import { qk } from "../../lib/events";

/** 后端 message 已面向用户；非 AppError 尽量保底不抛裸异常文案。 */
export function errorMessage(e: unknown): string {
  if (isAppError(e)) return e.message;
  if (typeof e === "string" && e.trim()) return e;
  if (e instanceof Error && e.message) return e.message;
  return "Something went wrong";
}

export function useActionError(): {
  error: string | null;
  /** 执行异步动作；成功返回其结果，失败展示错误并返回 null。 */
  run: <T>(fn: () => Promise<T>) => Promise<T | null>;
  /** 非点击路径（如乐观更新回滚）直接写入错误文案。 */
  fail: (message: string) => void;
  dismiss: () => void;
} {
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(async <T,>(fn: () => Promise<T>): Promise<T | null> => {
    try {
      const value = await fn();
      setError(null);
      return value;
    } catch (e) {
      setError(errorMessage(e));
      return null;
    }
  }, []);
  const fail = useCallback((message: string) => setError(message), []);
  const dismiss = useCallback(() => setError(null), []);
  return { error, run, fail, dismiss };
}

/** 任务写入后的补充失效（事件矩阵见 lib/events.ts）。 */
export function invalidateTasks(qc: QueryClient, cycleId: string): void {
  void qc.invalidateQueries({ queryKey: qk.editorWorkspace(cycleId) });
  // A parent goal link changes the work mix of its descendants too.
  void qc.invalidateQueries({ queryKey: ["editor-workspace"] });
  void qc.invalidateQueries({ queryKey: ["editor-workspaces"] });
  void qc.invalidateQueries({ queryKey: qk.previewSummary(cycleId) });
  void qc.invalidateQueries({ queryKey: qk.plannerState() });
}

/** 周期写入影响整棵周期树，从根失效全部编辑视图（粗但正确，见 events.ts）。 */
export function invalidateCycles(qc: QueryClient): void {
  void qc.invalidateQueries({ queryKey: qk.plannerState() });
  void qc.invalidateQueries({ queryKey: ["editor-workspace"] });
  void qc.invalidateQueries({ queryKey: ["editor-workspaces"] });
}
