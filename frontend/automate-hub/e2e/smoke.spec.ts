import { expect, test } from "./fixtures/greentic";
import { request as playwrightRequest } from "@playwright/test";

test("@smoke loads the real GUI and serves tokenized API requests", async ({
  page,
  api,
  baseUrl,
  expectNoRedErrors,
  snapshot,
}) => {
  await expect(page.getByRole("heading", { name: /Automate desktop tasks/i })).toBeVisible();
  await expect(page.getByRole("link", { name: /Start with a Prompt/i })).toBeVisible();

  const health = await api.get<{ apiVersion: string; status: string }>("/health");
  expect(health).toEqual({ apiVersion: "v1", status: "ok" });

  const setup = await api.get<{ items: Array<{ id: string; status: string }> }>("/setup/checklist");
  expect(setup.items.map((item) => item.id)).toContain("screen_capture_permission");

  const anonymous = await playwrightRequest.newContext({
    baseURL: baseUrl,
    extraHTTPHeaders: { accept: "application/json" },
  });
  const rejected = await anonymous.post("/api/v1/setup/fix", {
    data: { id: "runtime_home" },
  });
  expect(rejected.status()).toBe(403);
  const payload = (await rejected.json()) as { ok: boolean; error: { code: string } };
  expect(payload.ok).toBe(false);
  expect(payload.error.code).toBe("security.token_required");
  await anonymous.dispose();

  await expectNoRedErrors();
  await snapshot("smoke-home");
});
