export type DesktopPlatform = "macos" | "windows" | "linux" | "web";

export declare const DESKTOP_PLATFORMS: readonly DesktopPlatform[];
export declare const NATIVE_DESKTOP_PLATFORMS: readonly Exclude<DesktopPlatform, "web">[];

export declare function normalizePlatform(
  value: unknown,
  options?: { allowWeb?: boolean },
): DesktopPlatform;

export declare function platformFromTargetTriple(value: unknown): Exclude<DesktopPlatform, "web">;

export declare function resolveBuildPlatform(options?: {
  env?: Record<string, string | undefined>;
  requireNative?: boolean;
}): DesktopPlatform;
