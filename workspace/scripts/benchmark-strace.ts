#!/usr/bin/env node

import { spawnSync } from "node:child_process";

import { workspacePaths } from "./workspace-paths.ts";

class ScriptError extends Error {
  readonly exitCode: number;

  constructor(message: string, exitCode = 1) {
    super(message);
    this.name = "ScriptError";
    this.exitCode = exitCode;
  }
}

function runCommand(command: string, commandArguments: string[]) {
  const result = spawnSync(command, commandArguments, {
    cwd: workspacePaths.sourceRoot,
    encoding: "utf8",
    stdio: "inherit",
  });

  if (result.status !== 0) {
    throw new ScriptError(`command failed (${result.status ?? 1}): ${command} ${commandArguments.join(" ")}`);
  }
}

function requireNoArguments(argumentsList: string[]) {
  if (argumentsList.length !== 0) {
    throw new ScriptError(`unknown argument: ${argumentsList[0]}`);
  }
}

function runStrace() {
  runCommand("strace", ["--version"]);
  runCommand("cargo", ["build", "-p", "rbusd", "-p", "rbus-bench"]);
  runCommand("node", [workspacePaths.hotPathSyscallSmokeScript]);
  runCommand(workspacePaths.rbusBenchBinary, [
    "--suite",
    "--matrix-smoke",
    "--messages",
    "1",
    "--format",
    "csv",
    "--no-gate",
  ]);
}

try {
  requireNoArguments(process.argv.slice(2));
  runStrace();
} catch (error) {
  if (error instanceof Error) {
    console.error(error.stack ?? error.message);
  } else {
    console.error(error);
  }
  process.exit(error instanceof ScriptError ? error.exitCode : 1);
}
