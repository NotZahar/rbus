#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { workspacePaths } from "./workspace-paths.ts";

const defaultOutputPath = "benches/baselines/rbus_suite_baseline_x86_64.csv";
const defaultMessages = "100000";
const defaultRuns = "1";
const csvFieldCount = 36;
const latencyThresholdMultiplier = 2;
const latencyThresholdPaddingNs = 50_000;
const throughputThresholdMultiplier = 0.8;
const syscallThresholdMultiplier = 1.25;
const syscallThresholdPadding = 16;
const statusFieldIndex = 0;
const statusOk = "ok";
const syscallsPerMessageFieldIndex = 28;
const throughputMessagesPerSecondFieldIndex = 22;
const payloadThroughputGibPerSecondFieldIndex = 23;
const latencyFieldIndices = [8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21] as const;

class ScriptError extends Error {
  readonly exitCode: number;

  constructor(message: string, exitCode = 1) {
    super(message);
    this.name = "ScriptError";
    this.exitCode = exitCode;
  }
}

type ReferenceOptions = {
  messages: string;
  outputPath: string;
  runs: string;
  matrixSmoke: boolean;
};

function parseArguments(argumentsList: string[]): ReferenceOptions {
  const options: ReferenceOptions = {
    messages: defaultMessages,
    outputPath: defaultOutputPath,
    runs: defaultRuns,
    matrixSmoke: false,
  };

  for (let index = 0; index < argumentsList.length; index += 1) {
    const argument = argumentsList[index];
    switch (argument) {
      case "--messages":
        index += 1;
        options.messages = requireValue(argumentsList[index], argument);
        break;
      case "--output":
        index += 1;
        options.outputPath = requireValue(argumentsList[index], argument);
        break;
      case "--runs":
        index += 1;
        options.runs = requireValue(argumentsList[index], argument);
        break;
      case "--matrix-smoke":
        options.matrixSmoke = true;
        break;
      default:
        throw new ScriptError(`unknown argument: ${argument}`);
    }
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

function runCommand(command: string, commandArguments: string[], captureStdout: boolean) {
  const result = spawnSync(command, commandArguments, {
    cwd: workspacePaths.sourceRoot,
    encoding: "utf8",
    stdio: captureStdout ? ["ignore", "pipe", "pipe"] : "inherit",
  });

  if (result.status !== 0) {
    const stdout = result.stdout ? `\nstdout:\n${result.stdout}` : "";
    const stderr = result.stderr ? `\nstderr:\n${result.stderr}` : "";
    throw new ScriptError(`command failed (${result.status ?? 1}): ${command} ${commandArguments.join(" ")}${stdout}${stderr}`);
  }

  return result.stdout ?? "";
}

function runReference(options: ReferenceOptions) {
  runCommand("cargo", ["build", "--profile", "bench", "-p", "rbusd", "-p", "rbus-bench"], false);
  const suiteArguments = [
    "--suite",
    "--messages",
    options.messages,
    "--runs",
    options.runs,
    "--format",
    "csv",
    "--no-gate",
  ];
  if (options.matrixSmoke) {
    suiteArguments.splice(1, 0, "--matrix-smoke");
  }
  const csv = runCommand(workspacePaths.rbusBenchBinary, suiteArguments, true);
  const thresholdCsv = thresholdCsvFromReference(csv);
  const rows = validateCsv(thresholdCsv);
  const outputPath = resolve(workspacePaths.sourceRoot, options.outputPath);
  writeFileSync(outputPath, thresholdCsv);
  console.log(JSON.stringify({
    status: "ok",
    output: outputPath,
    rows,
    messages: Number(options.messages),
    runs: Number(options.runs),
  }));
}

function validateCsv(csv: string) {
  const lines = csv.trimEnd().split("\n");
  if (lines.length < 2) {
    throw new ScriptError("reference suite produced an empty CSV");
  }
  if (!lines[0].startsWith("status,scenario,transport,wait_mode,payload_size,")) {
    throw new ScriptError(`unexpected suite CSV header:\n${lines[0]}`);
  }
  for (const line of lines.slice(1)) {
    const fields = line.split(",");
    if (fields.length !== csvFieldCount) {
      throw new ScriptError(`unexpected suite CSV field count:\n${line}`);
    }
    if (fields[statusFieldIndex] !== statusOk) {
      throw new ScriptError(`non-ok benchmark row:\n${line}`);
    }
  }
  return lines.length - 1;
}

function thresholdCsvFromReference(csv: string) {
  const lines = csv.trimEnd().split("\n");
  if (lines.length < 2) {
    throw new ScriptError("reference suite produced an empty CSV");
  }
  return `${lines[0]}\n${lines.slice(1).map(thresholdLineFromReference).join("\n")}\n`;
}

function thresholdLineFromReference(line: string) {
  const fields = line.split(",");
  if (fields.length !== csvFieldCount) {
    throw new ScriptError(`unexpected suite CSV field count:\n${line}`);
  }
  if (fields[statusFieldIndex] !== statusOk) {
    throw new ScriptError(`non-ok benchmark row:\n${line}`);
  }
  for (const fieldIndex of latencyFieldIndices) {
    fields[fieldIndex] = latencyThreshold(fields[fieldIndex]);
  }
  fields[throughputMessagesPerSecondFieldIndex] = throughputThreshold(
    fields[throughputMessagesPerSecondFieldIndex],
    6,
  );
  fields[payloadThroughputGibPerSecondFieldIndex] = throughputThreshold(
    fields[payloadThroughputGibPerSecondFieldIndex],
    9,
  );
  fields[syscallsPerMessageFieldIndex] = syscallThreshold(fields[syscallsPerMessageFieldIndex]);
  return fields.join(",");
}

function latencyThreshold(value: string) {
  const measuredValue = parseInteger(value);
  if (measuredValue === 0) {
    return value;
  }
  return String(
    Math.ceil(
      Math.max(
        measuredValue * latencyThresholdMultiplier,
        measuredValue + latencyThresholdPaddingNs,
      ),
    ),
  );
}

function throughputThreshold(value: string, fractionDigits: number) {
  const measuredValue = parseFiniteNumber(value);
  return (measuredValue * throughputThresholdMultiplier).toFixed(fractionDigits);
}

function syscallThreshold(value: string) {
  const measuredValue = parseFiniteNumber(value);
  if (measuredValue === 0) {
    return value;
  }
  return (measuredValue * syscallThresholdMultiplier + syscallThresholdPadding).toFixed(6);
}

function parseInteger(value: string) {
  if (!/^[0-9]+$/.test(value)) {
    throw new ScriptError(`invalid integer field: ${value}`);
  }
  return Number.parseInt(value, 10);
}

function parseFiniteNumber(value: string) {
  const measuredValue = Number(value);
  if (!Number.isFinite(measuredValue) || measuredValue < 0) {
    throw new ScriptError(`invalid numeric field: ${value}`);
  }
  return measuredValue;
}

try {
  runReference(parseArguments(process.argv.slice(2)));
} catch (error) {
  if (error instanceof Error) {
    console.error(error.stack ?? error.message);
  } else {
    console.error(error);
  }
  process.exit(error instanceof ScriptError ? error.exitCode : 1);
}
