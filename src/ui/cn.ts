/**
 * Minimal className joiner — enough for conditional classes without pulling
 * in a runtime dependency. Falsy parts are skipped.
 */
export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
