import fs from "node:fs";
import path from "node:path";

/**
 * Resolve npm's JavaScript entry point so Node can invoke it directly on every
 * platform. Windows exposes npm as npm.cmd, so the resolved file must be the
 * real npm-cli.js entry point rather than an arbitrary executable on PATH.
 */
function realNpmCliPath(candidate) {
  if (typeof candidate !== "string" || !candidate.trim()) return null;
  let resolved;
  try {
    resolved = fs.realpathSync(candidate);
  } catch {
    return null;
  }
  if (path.basename(resolved) !== "npm-cli.js") return null;
  if (path.basename(path.dirname(resolved)) !== "bin") return null;
  if (path.basename(path.dirname(path.dirname(resolved))) !== "npm") return null;
  return resolved;
}

export function resolveNpmCliPath({
  env = process.env,
  execPath = process.execPath,
  allowMissing = false,
} = {}) {
  const candidates = [
    env.npm_execpath,
    path.join(path.dirname(execPath), "node_modules", "npm", "bin", "npm-cli.js"),
    path.resolve(path.dirname(execPath), "..", "lib", "node_modules", "npm", "bin", "npm-cli.js"),
    path.resolve(
      path.dirname(execPath),
      "..",
      "libexec",
      "lib",
      "node_modules",
      "npm",
      "bin",
      "npm-cli.js",
    ),
  ].filter(Boolean);
  const cli = candidates.map(realNpmCliPath).find(Boolean);
  if (!cli && !allowMissing) throw new Error("npm CLI JavaScript entrypoint not found");
  return cli;
}

export function npmCliInvocation(args = [], options = {}) {
  const execPath = options.execPath ?? process.execPath;
  const cli = resolveNpmCliPath({ ...options, execPath });
  if (!cli) return null;
  return { command: execPath, args: [cli, ...args] };
}
