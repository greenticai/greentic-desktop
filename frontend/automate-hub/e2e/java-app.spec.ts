import type { APIResponse, Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "./fixtures/greentic";

const fixtureRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
  "fixtures",
  "java",
  "customer-form",
);

const javaCustomerFormDraft = JSON.stringify({
  runner_id: "java.customer_form",
  version: "0.1.0-draft",
  summary: "Enter a customer in the Java customer form and return the confirmation id",
  risk_level: "medium",
  required_capabilities: [
    "java.find_window",
    "java.find_component",
    "java.type_text",
    "java.click_component",
    "java.read_text",
  ],
  inputs: {
    customer_name: { type: "string", required: true },
    email: { type: "string", required: true },
  },
  outputs: {
    confirmation_id: { type: "string" },
  },
  steps: [
    { id: "open", action: "find_window", required_capability: "java.find_window" },
    { id: "name", action: "type_text", required_capability: "java.type_text" },
    { id: "email", action: "type_text", required_capability: "java.type_text" },
    { id: "save", action: "click_component", required_capability: "java.click_component" },
    { id: "read", action: "read_text", required_capability: "java.read_text" },
  ],
  assertions: ["confirmation_id is visible"],
  open_questions: [],
});

type PlannerDraft = {
  draftId: string;
  runnerId: string;
  requiredAdapters: string[];
  inputs: string[];
  outputs: string[];
};

type ExtensionHealth = {
  status: string;
  message: string;
};

test.describe("@java Java accessibility adapter", () => {
  test.use({ testPermissions: "all_ready" });

  test("@java searches, installs, and health-checks the Java adapter", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    await page.goto(appUrl(guiUrl, "/settings"));
    await page.getByPlaceholder("Search extensions").fill("java");

    const javaRow = page.getByTestId("extension-greentic.desktop.java-accessibility");
    await expect(javaRow).toBeVisible();
    await clickApiAction(page, "/api/v1/extensions/install", () =>
      javaRow.getByRole("button", { name: /install/i }).click(),
    );
    await expect(javaRow.getByText("Installed")).toBeVisible();

    const healthResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/extensions/greentic.desktop.java-accessibility/health") &&
        response.request().method() === "POST",
    );
    await javaRow.getByRole("button", { name: /test/i }).click();
    const health = await expectData<ExtensionHealth>(await healthResponse);
    expect(health.status).toBe("healthy");
    expect(health.message).toContain("healthy");
    await expect(page.getByText(/greentic\.desktop\.java-accessibility: healthy/i)).toBeVisible();
    await expectNoRedErrors();

    const installed = javaRow.getByText("Installed");
    await expect(installed).toBeVisible();
  });
});

test.describe("@java Java customer form prompt runner", () => {
  test.use({ llmMockDraftJson: javaCustomerFormDraft, testPermissions: "all_ready" });

  test("@java creates and runs a generic Java customer form runner", async ({
    page,
    api,
    guiUrl,
    expectNoRedErrors,
  }) => {
    await api.post("/extensions/install", {
      source: "store://greentic.desktop.java-accessibility",
    });
    await api.put("/settings/llm", {
      provider: "openai_compatible",
      model: "fixture-java-planner",
      endpoint: "http://127.0.0.1:9/v1",
    });

    await page.goto(appUrl(guiUrl, "/create", { mode: "prompt" }));
    await page
      .getByTestId("prompt-textarea")
      .fill(
        "Open the Java customer form, enter customer_name and email, click Save, and return confirmation_id",
      );
    const draftResponse = page.waitForResponse(
      (response) =>
        response.url().includes("/api/v1/planner/drafts") &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: /Generate Draft Runner/i }).click();
    const draft = await expectData<PlannerDraft>(await draftResponse);
    expect(draft.runnerId).toBe("java.customer_form");
    expect(draft.requiredAdapters).toContain("greentic.desktop.java-accessibility");
    expect(draft.inputs).toEqual(["customer_name", "email"]);
    expect(draft.outputs).toEqual(["confirmation_id"]);

    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Continue/i }).click();
    await page.getByRole("button", { name: /Save Runner/i }).click();
    await page.getByRole("link", { name: /View Runner/i }).click();

    const runner = page.getByTestId("runner-java.customer_form");
    await expect(runner).toBeVisible();
    await runner.getByTestId("runner-java.customer_form-run").click();
    await runner.getByTestId("runner-java.customer_form-input-customer_name").fill("Alice Example");
    await runner.getByTestId("runner-java.customer_form-input-email").fill("alice@example.test");
    await runner.getByTestId("runner-java.customer_form-run-confirm").click();
    await expect(runner.getByText("confirmation_id")).toBeVisible();
    await expect(runner.getByText(/^CONF-[0-9a-f]{8}$/)).toBeVisible();
    await expectNoRedErrors();
  });
});

test.describe("@manual @java-real Java customer form fixture", () => {
  test("@manual @java-real builds the Swing fixture when explicitly enabled", async () => {
    test.skip(
      process.env.GREENTIC_DESKTOP_REAL_JAVA !== "1",
      "Set GREENTIC_DESKTOP_REAL_JAVA=1 and grant Java accessibility permissions to run the real Swing fixture.",
    );
    test.skip(!existsSync(path.join(fixtureRoot, "build.sh")), "Java fixture build script is missing.");
    execFileSync("./build.sh", { cwd: fixtureRoot, stdio: "inherit" });
    expect(existsSync(path.join(fixtureRoot, "build", "CustomerForm.class"))).toBe(true);
  });
});

function appUrl(guiUrl: string, pathname: string, params: Record<string, string> = {}) {
  const url = new URL(guiUrl);
  url.pathname = pathname;
  for (const [key, value] of Object.entries(params)) {
    url.searchParams.set(key, value);
  }
  return url.toString();
}

async function clickApiAction(page: Page, path: string, action: () => Promise<unknown>) {
  const responsePromise = page.waitForResponse(
    (response) =>
      response.url().includes(path) && response.request().method() === "POST",
  );
  await action();
  await expectOk(await responsePromise);
}

async function expectOk(response: APIResponse) {
  expect(response.ok()).toBe(true);
  const payload = (await response.json()) as { ok: boolean; error?: { message: string } };
  expect(payload.ok, payload.error?.message).toBe(true);
}

async function expectData<T>(response: { ok: () => boolean; json: () => Promise<unknown> }) {
  expect(response.ok()).toBe(true);
  const payload = (await response.json()) as { ok: boolean; data: T; error?: { message: string } };
  expect(payload.ok, payload.error?.message).toBe(true);
  return payload.data;
}
