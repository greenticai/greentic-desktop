import { expect, request as playwrightRequest, test as base } from "@playwright/test";
import type { APIRequestContext, TestInfo } from "@playwright/test";
import { execFileSync, spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

type ApiHelper = {
  get: <T = unknown>(path: string) => Promise<T>;
  post: <T = unknown>(path: string, body?: unknown) => Promise<T>;
  put: <T = unknown>(path: string, body?: unknown) => Promise<T>;
  patch: <T = unknown>(path: string, body?: unknown) => Promise<T>;
  raw: APIRequestContext;
};

type GreenticFixtures = {
  api: ApiHelper;
  baseUrl: string;
  guiUrl: string;
  processLogs: string;
  runtimeHome: string;
  expectNoRedErrors: () => Promise<void>;
  snapshot: (label: string) => Promise<void>;
};

type GreenticOptions = {
  llmMockDraftJson?: string;
  llmMockDraftJsonSequence?: string;
  testPermissions?: string;
};

const fixtureDir = path.dirname(fileURLToPath(import.meta.url));
const frontendRoot = path.resolve(fixtureDir, "..", "..");
const repoRoot = path.resolve(frontendRoot, "..", "..");

export const test = base.extend<GreenticFixtures & GreenticOptions>({
  llmMockDraftJson: [undefined, { option: true }],
  llmMockDraftJsonSequence: [undefined, { option: true }],
  testPermissions: [undefined, { option: true }],

  runtimeHome: async ({}, use, testInfo) => {
    const runtimeHome = path.join(tmpdir(), `greentic-desktop-e2e-${process.pid}-${testInfo.workerIndex}-${Date.now()}`);
    mkdirSync(runtimeHome, { recursive: true });
    await use(runtimeHome);
    if (testInfo.status === testInfo.expectedStatus) {
      rmSync(runtimeHome, { recursive: true, force: true });
    }
  },

  processLogs: async ({}, use, testInfo) => {
    const logs = testInfo.outputPath("greentic-desktop.log");
    await use(logs);
  },

  guiUrl: async (
    { runtimeHome, processLogs, testPermissions, llmMockDraftJson, llmMockDraftJsonSequence },
    use,
    testInfo,
  ) => {
    const binary = ensureGreenticBinary();
    const process = spawnGreentic(binary, runtimeHome, processLogs, {
      llmMockDraftJson,
      llmMockDraftJsonSequence,
      testPermissions,
    });
    try {
      const guiUrl = await waitForGuiUrl(process, processLogs);
      await use(guiUrl);
    } finally {
      await stopGreentic(process);
      await attachIfExists(testInfo, "greentic-desktop.log", processLogs, "text/plain");
    }
  },

  baseUrl: async ({ guiUrl }, use) => {
    await use(new URL(guiUrl).origin);
  },

  api: async ({ baseUrl, guiUrl }, use) => {
    const token = new URL(guiUrl).searchParams.get("token") ?? "";
    const context = await playwrightRequest.newContext({
      baseURL: baseUrl,
      extraHTTPHeaders: {
        accept: "application/json",
        "x-greentic-gui-token": token,
      },
    });
    const helper = {
      raw: context,
      get: <T = unknown>(path: string) => json<T>(context.get(apiPath(path))),
      post: <T = unknown>(path: string, body?: unknown) => json<T>(context.post(apiPath(path), { data: body ?? {} })),
      put: <T = unknown>(path: string, body?: unknown) => json<T>(context.put(apiPath(path), { data: body ?? {} })),
      patch: <T = unknown>(path: string, body?: unknown) => json<T>(context.patch(apiPath(path), { data: body ?? {} })),
    };
    await use(helper);
    await context.dispose();
  },

  page: async ({ page, guiUrl, processLogs, runtimeHome, api }, use, testInfo) => {
    const consoleLines: string[] = [];
    const requestFailures: string[] = [];
    page.on("console", (message) => consoleLines.push(`[${message.type()}] ${message.text()}`));
    page.on("requestfailed", (request) => {
      requestFailures.push(`${request.method()} ${request.url()} ${request.failure()?.errorText ?? ""}`);
    });

    await page.goto(guiUrl);
    await use(page);

    if (testInfo.status !== testInfo.expectedStatus) {
      await attachText(testInfo, "browser-console.log", consoleLines.join("\n"));
      await attachText(testInfo, "request-failures.log", requestFailures.join("\n"));
      await attachText(testInfo, "runtime-tree.txt", runtimeTree(runtimeHome));
      await attachApiSnapshot(testInfo, api, "/health", "api-health.json");
      await attachApiSnapshot(testInfo, api, "/setup/checklist", "api-setup-checklist.json");
      await attachApiSnapshot(testInfo, api, "/runners", "api-runners.json");
      await attachIfExists(testInfo, "greentic-desktop.log", processLogs, "text/plain");
    }
  },

  expectNoRedErrors: async ({ page }, use) => {
    await use(async () => {
      await expect(page.getByText("A valid GUI session token is required")).toHaveCount(0);
      await expect(page.getByText("Action failed")).toHaveCount(0);
    });
  },

  snapshot: async ({ page, api }, use, testInfo) => {
    await use(async (label: string) => {
      const safe = label.replace(/[^a-z0-9._-]+/gi, "-").toLowerCase();
      await page.screenshot({ path: testInfo.outputPath(`${safe}.png`), fullPage: true });
      await attachApiSnapshot(testInfo, api, "/health", `${safe}-health.json`);
      await attachApiSnapshot(testInfo, api, "/setup/checklist", `${safe}-setup.json`);
    });
  },
});

export { expect };

async function json<T>(responsePromise: Promise<{ json: () => Promise<unknown>; ok: () => boolean; status: () => number }>): Promise<T> {
  const response = await responsePromise;
  const payload = (await response.json()) as { ok: boolean; data?: T; error?: { code: string; message: string } };
  if (!payload.ok) {
    throw new Error(`${payload.error?.code ?? response.status()}: ${payload.error?.message ?? "API request failed"}`);
  }
  return payload.data as T;
}

function ensureGreenticBinary(): string {
  if (process.env.GREENTIC_DESKTOP_BINARY) {
    return process.env.GREENTIC_DESKTOP_BINARY;
  }
  const binary = path.join(repoRoot, "target", "debug", process.platform === "win32" ? "greentic-desktop.exe" : "greentic-desktop");
  if (!existsSync(binary)) {
    execFileSync("cargo", ["build", "--bin", "greentic-desktop"], {
      cwd: repoRoot,
      stdio: "inherit",
    });
  }
  return binary;
}

function apiPath(pathname: string): string {
  if (pathname.startsWith("/api/v1/")) {
    return pathname;
  }
  return `/api/v1${pathname.startsWith("/") ? "" : "/"}${pathname}`;
}

function spawnGreentic(
  binary: string,
  runtimeHome: string,
  logPath: string,
  options: Pick<GreenticOptions, "llmMockDraftJson" | "llmMockDraftJsonSequence" | "testPermissions">,
): ChildProcessWithoutNullStreams {
  writeFileSync(logPath, "");
  const child = spawn(binary, ["gui", "--no-open", "--bind", "127.0.0.1:0"], {
    cwd: repoRoot,
    env: {
      ...process.env,
      GREENTIC_DESKTOP_E2E: "1",
      GREENTIC_DESKTOP_HOME: runtimeHome,
      ...(options.llmMockDraftJson
        ? { GREENTIC_DESKTOP_LLM_MOCK_DRAFT_JSON: options.llmMockDraftJson }
        : {}),
      ...(options.llmMockDraftJsonSequence
        ? { GREENTIC_DESKTOP_LLM_MOCK_DRAFT_JSON_SEQUENCE: options.llmMockDraftJsonSequence }
        : {}),
      ...(options.testPermissions ? { GREENTIC_DESKTOP_TEST_PERMISSIONS: options.testPermissions } : {}),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (chunk) => appendRedacted(logPath, chunk.toString()));
  child.stderr.on("data", (chunk) => appendRedacted(logPath, chunk.toString()));
  return child;
}

function waitForGuiUrl(child: ChildProcessWithoutNullStreams, logPath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const deadline = setTimeout(() => {
      reject(new Error(`Timed out waiting for Greentic GUI URL. See ${logPath}`));
    }, 30_000);
    child.once("exit", (code, signal) => {
      clearTimeout(deadline);
      reject(new Error(`Greentic GUI exited before startup. code=${code} signal=${signal}. See ${logPath}`));
    });
    child.stdout.on("data", (chunk) => {
      const match = chunk.toString().match(/Greentic Automate Hub: (http:\/\/127\.0\.0\.1:\d+\/\?token=[a-f0-9]+)/);
      if (match) {
        clearTimeout(deadline);
        resolve(match[1]);
      }
    });
  });
}

async function stopGreentic(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.exitCode != null) {
    return;
  }
  child.kill();
  await new Promise<void>((resolve) => {
    const timeout = setTimeout(() => {
      child.kill("SIGKILL");
      resolve();
    }, 3_000);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolve();
    });
  });
}

function appendRedacted(logPath: string, text: string): void {
  const redacted = text.replace(/token=[a-f0-9]+/g, "token=<redacted>");
  writeFileSync(logPath, redacted, { flag: "a" });
}

async function attachIfExists(testInfo: TestInfo, name: string, filePath: string, contentType: string): Promise<void> {
  if (existsSync(filePath)) {
    await testInfo.attach(name, { path: filePath, contentType });
  }
}

async function attachText(testInfo: TestInfo, name: string, body: string): Promise<void> {
  await testInfo.attach(name, { body: body || "(empty)", contentType: "text/plain" });
}

async function attachApiSnapshot(testInfo: TestInfo, api: ApiHelper, path: string, name: string): Promise<void> {
  try {
    const data = await api.get(path);
    await testInfo.attach(name, {
      body: JSON.stringify(data, null, 2),
      contentType: "application/json",
    });
  } catch (error) {
    await attachText(testInfo, `${name}.error.txt`, error instanceof Error ? error.message : String(error));
  }
}

function runtimeTree(root: string): string {
  const lines: string[] = [];
  walk(root, "", lines, 0);
  return lines.join("\n");
}

function walk(current: string, prefix: string, lines: string[], depth: number): void {
  if (depth > 3 || !existsSync(current)) {
    return;
  }
  for (const entry of readdirSync(current).slice(0, 80)) {
    const full = path.join(current, entry);
    const stat = statSync(full);
    lines.push(`${prefix}${entry}${stat.isDirectory() ? "/" : ` (${stat.size} bytes)`}`);
    if (stat.isDirectory()) {
      walk(full, `${prefix}  `, lines, depth + 1);
    }
  }
}
