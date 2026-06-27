#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const mountedWorkspaceRoot = resolve(repositoryRoot, "workspace");
const mountedSourceRoot = "/workspace/src";
const imageName = "localhost/rbus-dev:ubuntu-26";

function runCommand(command, commandArguments, options = {}) {
  const result = spawnSync(command, commandArguments, {
    cwd: repositoryRoot,
    stdio: options.stdio ?? "inherit",
    encoding: "utf8",
    env: { ...process.env, ...options.environment },
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  return result.stdout ?? "";
}

function currentUserId() {
  return runCommand("id", ["-u"], { stdio: "pipe" }).trim();
}

function currentGroupId() {
  return runCommand("id", ["-g"], { stdio: "pipe" }).trim();
}

function currentUserArguments(userId, groupId) {
  return ["--user", `${userId}:${groupId}`];
}

function containerArguments(commandLine) {
  mkdirSync(resolve(mountedWorkspaceRoot, ".cache", "cargo"), { recursive: true });
  const userId = currentUserId();
  const groupId = currentGroupId();
  const runtimeDirectory = `/run/user/${userId}`;
  return [
    "run",
    "--rm",
    "--userns=keep-id",
    ...currentUserArguments(userId, groupId),
    "--volume",
    `${mountedWorkspaceRoot}:/workspace`,
    "--volume",
    `${runtimeDirectory}:${runtimeDirectory}`,
    "--workdir",
    mountedSourceRoot,
    "--env",
    "CARGO_HOME=/workspace/.cache/cargo",
    "--env",
    "RUSTUP_HOME=/opt/rustup",
    imageName,
    "zsh",
    "-lc",
    commandLine,
  ];
}

function runInContainer(commandLine) {
  runCommand("podman", containerArguments(commandLine));
}

function buildImage() {
  runCommand("podman", [
    "build",
    "--format",
    "docker",
    "--tag",
    imageName,
    "--file",
    "Containerfile",
    ".",
  ]);
}

function printToolchain() {
  runInContainer("rustc --version && cargo --version && rustup show active-toolchain");
}

function runCargo(commandArguments) {
  const escapedArguments = commandArguments.map((argument) => `'${argument.replaceAll("'", "'\\''")}'`);
  runInContainer(`cargo ${escapedArguments.join(" ")}`);
}

function runCheckSuite() {
  runInContainer("cargo fmt --all -- --check");
  runInContainer("cargo clippy --workspace --all-targets --all-features -- -D warnings");
  runInContainer("cargo test --workspace --all-features");
  runInContainer("cargo test --workspace --no-default-features");
  runInContainer("RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps");
  runInContainer("cargo run -p rbus-bench -- --smoke");
  runBenchSuiteSmoke();
  runInContainer("cargo run -p rbus-cli -- --help >/tmp/rbus-cli-help.txt && test -s /tmp/rbus-cli-help.txt");
}

function runBenchSmoke() {
  runInContainer("cargo run -p rbus-bench -- --smoke");
}

function runBenchSuiteSmoke() {
  runInContainer("cargo build -p rbusd && cargo run -p rbus-bench -- --suite --matrix-smoke --messages 1 --format csv --no-gate");
}

function runHotPathSyscallSmoke() {
  runInContainer("node /workspace/scripts/hot-path-syscall-smoke.ts");
}

function printUsage() {
  const packageMetadata = readFileSync(resolve(repositoryRoot, "package.json"), "utf8");
  const packageName = JSON.parse(packageMetadata).name;
  console.log(`${packageName}: commands`);
  console.log("  image                  build the podman image");
  console.log("  toolchain              print the Rust toolchain inside the container");
  console.log("  shell                  open a shell inside the container");
  console.log("  cargo <args...>        run cargo inside the container");
  console.log("  fmt                    cargo fmt --all -- --check");
  console.log("  clippy                 cargo clippy --workspace --all-targets --all-features -- -D warnings");
  console.log("  test                   cargo test --workspace --all-features");
  console.log("  test-no-default        cargo test --workspace --no-default-features");
  console.log("  doc                    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps");
  console.log("  bench-smoke            short benchmark smoke");
  console.log("  bench-suite-smoke      short multi-process benchmark suite smoke");
  console.log("  hot-path-syscall-smoke strace regression for 0 syscalls in the hot path");
  console.log("  check                  full check suite");
}

const [command, ...commandArguments] = process.argv.slice(2);

switch (command) {
  case "image":
    buildImage();
    break;
  case "toolchain":
    printToolchain();
    break;
  case "shell":
    runCommand("podman", containerArguments("zsh -i"));
    break;
  case "cargo":
    runCargo(commandArguments);
    break;
  case "fmt":
    runInContainer("cargo fmt --all -- --check");
    break;
  case "clippy":
    runInContainer("cargo clippy --workspace --all-targets --all-features -- -D warnings");
    break;
  case "test":
    runInContainer("cargo test --workspace --all-features");
    break;
  case "test-no-default":
    runInContainer("cargo test --workspace --no-default-features");
    break;
  case "doc":
    runInContainer("RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps");
    break;
  case "bench-smoke":
    runBenchSmoke();
    break;
  case "bench-suite-smoke":
    runBenchSuiteSmoke();
    break;
  case "hot-path-syscall-smoke":
    runHotPathSyscallSmoke();
    break;
  case "check":
    runCheckSuite();
    runHotPathSyscallSmoke();
    break;
  default:
    printUsage();
    break;
}
