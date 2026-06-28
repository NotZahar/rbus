#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { workspacePaths } from "./workspace-paths.ts";

const parseStatusMissingBegin = 10;
const parseStatusMissingEnd = 11;
const parseStatusUnterminatedInterval = 12;
const parseStatusSyscallsFound = 13;
const operations = ["publish", "try-recv"] as const;

type Operation = (typeof operations)[number];

class ScriptError extends Error {
  readonly exitCode: number;

  constructor(message: string, exitCode = 1) {
    super(message);
    this.name = "ScriptError";
    this.exitCode = exitCode;
  }
}

function runCommand(command: string, commandArguments: string[], options = { captureStdout: false }) {
  const result = spawnSync(command, commandArguments, {
    cwd: workspacePaths.sourceRoot,
    encoding: "utf8",
    stdio: options.captureStdout ? ["ignore", "pipe", "pipe"] : "inherit",
  });

  if (result.status !== 0) {
    const stderr = result.stderr ? `\nstderr:\n${result.stderr}` : "";
    const stdout = result.stdout ? `\nstdout:\n${result.stdout}` : "";
    throw new ScriptError(`command failed (${result.status ?? 1}): ${command} ${commandArguments.join(" ")}${stdout}${stderr}`);
  }

  return result.stdout ?? "";
}

function requireTool(toolName: string) {
  const result = spawnSync(toolName, ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new ScriptError(`required tool is missing: ${toolName}`);
  }
}

function runProbe(operation: Operation, temporaryDirectory: string) {
  const traceFile = join(temporaryDirectory, `rbus-${operation}-strace.log`);
  const stdout = runCommand(
    "strace",
    [
      "--quiet=all",
      "--trace=all",
      `--output=${traceFile}`,
      "--",
      workspacePaths.rbusBenchBinary,
      "--hot-path-syscall-probe",
      operation,
    ],
    { captureStdout: true },
  );

  assertProbeOutput(operation, stdout);
  assertZeroSyscallsBetweenMarkers(operation, traceFile);
  console.log(JSON.stringify({ operation, hot_path_syscalls: 0 }));
}

function assertProbeOutput(operation: Operation, stdout: string) {
  if (!stdout.includes("\"status\":\"ok\"")) {
    throw new ScriptError(`probe ${operation} did not report ok status\nstdout:\n${stdout}`);
  }
  if (!stdout.includes(`"operation":"${operation}"`)) {
    throw new ScriptError(`probe ${operation} stdout does not contain operation field\nstdout:\n${stdout}`);
  }
}

function assertZeroSyscallsBetweenMarkers(operation: Operation, traceFile: string) {
  const trace = readFileSync(traceFile, "utf8");
  const lines = trace.split("\n");
  const beginMarker = `write(1, "RBUS_HOT_PATH_BEGIN:${operation}`;
  const endMarker = `write(1, "RBUS_HOT_PATH_END:${operation}`;
  let insideInterval = false;
  let sawBegin = false;
  let sawEnd = false;
  const violations: string[] = [];

  for (const line of lines) {
    if (line.includes(beginMarker)) {
      insideInterval = true;
      sawBegin = true;
      continue;
    }
    if (line.includes(endMarker)) {
      insideInterval = false;
      sawEnd = true;
      continue;
    }
    if (insideInterval && line.length > 0) {
      violations.push(line);
    }
  }

  if (!sawBegin) {
    throw new ScriptError(`missing BEGIN marker for ${operation}\n${trace}`, parseStatusMissingBegin);
  }
  if (insideInterval) {
    throw new ScriptError(`unterminated hot path interval for ${operation}\n${trace}`, parseStatusUnterminatedInterval);
  }
  if (!sawEnd) {
    throw new ScriptError(`missing END marker for ${operation}\n${trace}`, parseStatusMissingEnd);
  }
  if (violations.length !== 0) {
    throw new ScriptError(
      `syscalls detected in ${operation} hot path:\n${violations.join("\n")}`,
      parseStatusSyscallsFound,
    );
  }
}

function main() {
  const temporaryDirectory = mkdtempSync(join(tmpdir(), "rbus-hot-path-syscall-"));
  try {
    requireTool("strace");
    runCommand("cargo", ["build", "-p", "rbusd", "-p", "rbus-bench"]);
    for (const operation of operations) {
      runProbe(operation, temporaryDirectory);
    }
  } finally {
    rmSync(temporaryDirectory, { force: true, recursive: true });
  }
}

try {
  main();
} catch (error) {
  if (error instanceof ScriptError) {
    console.error(error.stack ?? error.message);
    process.exit(error.exitCode);
  }
  if (error instanceof Error) {
    console.error(error.stack ?? error.message);
    process.exit(1);
  }
  console.error(error);
  process.exit(1);
}
