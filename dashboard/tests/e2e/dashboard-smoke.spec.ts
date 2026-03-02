import { expect, test } from "@playwright/test";

test("login page smoke", async ({ page }) => {
  await page.goto("/login");
  await expect(page.getByRole("heading", { name: "Admin sign in" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
});
