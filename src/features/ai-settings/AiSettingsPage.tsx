/**
 * AI 模型设置页（tasks §5.1，design D6）：说明行（接入方式与费用来源）
 * + 左栏供应商列表 + 右栏详情/表单的三层结构；布局取自 zcode 参考截图，
 * 样式全部按本仓 design.md 语义 token（页面外层 bg-subtle 画布承托两块
 * bg-content 白面板、1px border-light、方角、无阴影）。
 *
 * 查询键说明：后端没有 AI 设置事件（lib/events.ts 的失效矩阵不覆盖本
 * 页），所以这里直接使用字面量 key `["ai-settings"]`——events.ts 的 qk
 * 工厂不在本任务允许清单内。所有 mutator 经 onChanged 回调请求本页
 * invalidate 该 key；字面量 key 仅本页消费，待协调者稍后收敛进 qk 工厂。
 *
 * 入口契约：后续由协调者把 SettingsDialog 的 AI 分区接到本组件
 * （`<AiSettingsPage />`）；组件自取数、自管理选中态。
 */
import { useCallback, useEffect, useState, type JSX } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { EmptyState, ProgressDot, cn } from "../../ui";
import { getAiSettings, isAppError } from "../../lib/ipc";
import type { AiSettingsSummary } from "../../lib/ipc";
import { ProviderForm } from "./ProviderForm";

/** 仅本页消费的字面量查询键（见文件头注释）。 */
const AI_SETTINGS_KEY = ["ai-settings"] as const;

type Selection = { kind: "provider"; id: string } | { kind: "new" } | null;

function messageOf(error: unknown): string {
  return isAppError(error)
    ? error.message
    : error instanceof Error
      ? error.message
      : String(error);
}

export function AiSettingsPage(): JSX.Element {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery({
    queryKey: AI_SETTINGS_KEY,
    queryFn: getAiSettings,
  });

  // 写操作成功后的失效通知；key 只在本页出现（见文件头注释）。
  const invalidate = useCallback(
    () => queryClient.invalidateQueries({ queryKey: AI_SETTINGS_KEY }),
    [queryClient],
  );

  const [selection, setSelection] = useState<Selection>(null);

  const summary: AiSettingsSummary = settingsQuery.data ?? {
    active_provider: null,
    providers: [],
    ai_available: false,
  };

  // 选中态自愈：初次数据到达时选中激活项（否则第一项）；选中项被删除后
  // 回落到剩余第一项或空态——删除激活供应商后后端本就不会自动挑替代
  // （task 1.5），这里的回落只是列表浏览位置。
  useEffect(() => {
    const data = settingsQuery.data;
    if (data === undefined) return;
    setSelection((current) => {
      if (current === null) {
        const preferred = data.providers.find((p) => p.is_active) ?? data.providers[0];
        return preferred ? { kind: "provider", id: preferred.id } : null;
      }
      if (
        current.kind === "provider" &&
        !data.providers.some((p) => p.id === current.id)
      ) {
        const next = data.providers[0];
        return next ? { kind: "provider", id: next.id } : null;
      }
      return current;
    });
  }, [settingsQuery.data]);

  const selected =
    selection?.kind === "provider"
      ? (summary.providers.find((p) => p.id === selection.id) ?? null)
      : null;

  const select = (id: string) => setSelection({ kind: "provider", id });

  return (
    <div className="flex flex-col gap-3">
      {/* 说明行（task 5.1/§9.3）：先说明接入方式与费用来源，再给当前状态。 */}
      <header className="flex flex-col gap-1">
        <h3 className="text-block-title font-semibold text-primary">AI 模型设置</h3>
        <p className="text-caption text-secondary">
          BYOK — requests go directly to your provider using your own API key;
          you pay your provider directly.
        </p>
        <p className="flex items-center gap-1.5 text-caption text-secondary">
          <ProgressDot tone={summary.ai_available ? "active" : "idle"} />
          {summary.active_provider
            ? `当前激活：${summary.active_provider.name}`
            : "未配置 — 先添加并激活供应商，之后 AI 功能可用"}
        </p>
        {settingsQuery.isError && (
          <p className="text-caption text-danger">{messageOf(settingsQuery.error)}</p>
        )}
      </header>

      {/* 两栏：外层 bg-subtle 画布 + 两块 bg-content 面板（design D6 映射）。 */}
      <div className="flex items-stretch gap-2 bg-subtle p-2">
        <aside className="flex w-[240px] shrink-0 flex-col border border-light bg-content">
          <div className="border-b border-light px-3 py-2 text-block-title font-semibold text-primary">
            供应商
          </div>
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
            {settingsQuery.isPending ? (
              <p className="px-3 py-6 text-center text-caption text-hint">加载中…</p>
            ) : summary.providers.length === 0 ? (
              <p className="px-3 py-6 text-center text-caption text-hint">暂无供应商</p>
            ) : (
              summary.providers.map((item) => {
                const isSelected =
                  selection?.kind === "provider" && selection.id === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => select(item.id)}
                    aria-current={isSelected ? "true" : undefined}
                    className={cn(
                      "flex w-full items-start gap-2 border-b border-light px-3 py-2 text-left",
                      "transition-colors duration-100 hover:bg-hover",
                      isSelected && "bg-focus-surface",
                    )}
                  >
                    {/* 状态点附文字说明（task 5.5），激活态不只靠颜色。 */}
                    <ProgressDot
                      tone={item.is_active ? "active" : "idle"}
                      className="mt-1.5"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-body text-primary">
                        {item.name}
                      </span>
                      <span className="block text-caption text-secondary">
                        {item.models.length} 个模型{item.is_active ? " · 激活" : ""}
                      </span>
                    </span>
                  </button>
                );
              })
            )}
          </div>
          <button
            type="button"
            onClick={() => setSelection({ kind: "new" })}
            className={cn(
              "flex w-full items-center gap-2 border-t border-light px-3 py-2 text-left",
              "text-menu text-primary transition-colors duration-100 hover:bg-hover",
              selection?.kind === "new" && "bg-focus-surface",
            )}
          >
            + 添加供应商
          </button>
        </aside>

        <section className="min-w-0 flex-1 border border-light bg-content">
          {settingsQuery.isPending ? (
            <p className="px-4 py-6 text-caption text-hint">加载中…</p>
          ) : selection?.kind === "new" ? (
            <ProviderForm
              key="new"
              provider={null}
              onSaved={select}
              onChanged={invalidate}
            />
          ) : selected ? (
            <ProviderForm
              key={selected.id}
              provider={selected}
              onSaved={select}
              onChanged={invalidate}
            />
          ) : (
            <EmptyState
              title="AI 供应商"
              description="从左栏「+ 添加供应商」开始：云厂商、网关或本地运行时（Ollama、LM Studio 等）都适用；添加后将其中一个设为激活，AI 功能即可使用。"
              className="m-4"
            />
          )}
        </section>
      </div>
    </div>
  );
}
