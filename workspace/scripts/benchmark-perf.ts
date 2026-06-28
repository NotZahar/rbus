#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

import { workspacePaths } from "./workspace-paths.ts";

const defaultBaselinePath = "benches/baselines/rbus_suite_baseline_x86_64.csv";
const defaultFormat = "json";
const defaultMessages = "32";
const defaultRuns = "5";

class ScriptError extends Error {
  readonly exitCode: number;

  constructor(message: string, exitCode = 1) {
    super(message);
    this.name = "ScriptError";
    this.exitCode = exitCode;
  }
}

type PerfOptions = {
  baselinePath: string;
  format: string;
  matrixSmoke: boolean;
  messages: string;
  runs: string;
};

function parseArguments(argumentsList: string[]): PerfOptions {
  const options: PerfOptions = {
    baselinePath: defaultBaselinePath,
    format: defaultFormat,
    matrixSmoke: false,
    messages: defaultMessages,
    runs: defaultRuns,
  };

  for (let index = 0; index < argumentsList.length; index += 1) {
    const argument = argumentsList[index];
    switch (argument) {
      case "--baseline":
        index += 1;
        options.baselinePath = requireValue(argumentsList[index], argument);
        break;
      case "--format":
        index += 1;
        options.format = requireValue(argumentsList[index], argument);
        break;
      case "--matrix-smoke":
        options.matrixSmoke = true;
        break;
      case "--messages":
        index += 1;
        options.messages = requireValue(argumentsList[index], argument);
        break;
      case "--runs":
        index += 1;
        options.runs = requireValue(argumentsList[index], argument);
        break;
      default:
        throw new ScriptError(`unknown argument: ${argument}`);
    }
  }

  if (options.format !== "json" && options.format !== "csv") {
    throw new ScriptError(`invalid --format value: ${options.format}`);
  }
  if (!/^[1-9][0-9]*$/.test(options.messages)) {
    throw new ScriptError(`invalid --messages value: ${options.messages}`);
  }
  if (!/^[1-9][0-9]*$/.test(options.runs)) {
    throw new ScriptError(`invalid --runs value: ${options.runs}`);
  }
  return options;
}

function requireValue(value: string | undefined, optionName: string): string {
  if (value === undefined || value.length === 0) {
    throw new ScriptError(`missing value for ${optionName}`);
  }
  return value;
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

function runPerf(options: PerfOptions) {
  runCommand("cargo", ["build", "-p", "rbusd", "-p", "rbus-bench"]);
  const suiteArguments = [
    "--suite",
    "--messages",
    options.messages,
    "--runs",
    options.runs,
    "--format",
    options.format,
    "--baseline",
    resolve(workspacePaths.sourceRoot, options.baselinePath),
  ];
  if (options.matrixSmoke) {
    suiteArguments.splice(1, 0, "--matrix-smoke");
  }
  runCommand(workspacePaths.rbusBenchBinary, suiteArguments);
}

try {
  runPerf(parseArguments(process.argv.slice(2)));
} catch (error) {
  if (error instanceof Error) {
    console.error(error.stack ?? error.message);
  } else {
    console.error(error);
  }
  process.exit(error instanceof ScriptError ? error.exitCode : 1);
}
