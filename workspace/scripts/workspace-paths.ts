import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsRoot = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = resolve(scriptsRoot, "..");
const sourceRoot = resolve(workspaceRoot, "src");

export const workspacePaths = {
  scriptsRoot,
  sourceRoot,
  rbusBenchBinary: resolve(sourceRoot, "target", "debug", "rbus-bench"),
  hotPathSyscallSmokeScript: resolve(scriptsRoot, "hot-path-syscall-smoke.ts"),
} as const;
