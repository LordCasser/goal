import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { isAppError } from "./lib/ipc";

/**
 * Placeholder shell.
 *
 * It exists to prove the full chain works end to end (React -> IPC -> Rust ->
 * SQLite -> back) before the real workspace is built. Replace it with the
 * planner workspace described in openspec/specs/planner-workspace/spec.md.
 */
export default function App() {
  const { data, error, isPending } = useQuery({
    queryKey: ["schema-version"],
    queryFn: () => invoke<number>("get_schema_version"),
  });

  const status = isPending
    ? "connecting…"
    : error
      ? `error: ${isAppError(error) ? `${error.code} — ${error.message}` : String(error)}`
      : `schema v${data}`;

  return (
    <div className="flex h-full flex-col bg-ink-25">
      <header className="flex h-[var(--app-header-height)] shrink-0 items-center justify-between border-b border-ink-100 px-3">
        <span className="text-[12px] font-bold uppercase tracking-[0.02em] text-ink-800">
          Planner
        </span>
        <span className="text-[12px] text-ink-500">{status}</span>
      </header>
      <main className="flex flex-1 items-center justify-center p-6">
        <div className="max-w-[366px] border border-dashed border-ink-300 p-6 text-center">
          <h2 className="m-0 text-[16px] font-bold uppercase text-ink-800">No cycles yet</h2>
          <p className="mt-2 mb-4 text-ink-600">
            The workspace is not built yet. Requirements live in{" "}
            <code className="font-mono">openspec/specs/</code>.
          </p>
        </div>
      </main>
    </div>
  );
}
