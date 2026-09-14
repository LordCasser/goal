/**
 * The single place that talks to the Rust side.
 *
 * Command names are declared once, here, so the IPC surface can be grepped.
 * Rust returns failures as `{ code, message }` — see `docs/architecture.md`.
 */
export type AppError = { code: string; message: string };

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export const commands = {
  getSchemaVersion: "get_schema_version",
} as const;
