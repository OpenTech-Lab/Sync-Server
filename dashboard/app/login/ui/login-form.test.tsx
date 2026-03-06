import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";

import { LoginForm } from "./login-form";

const pushMock = vi.fn();
const refreshMock = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    push: pushMock,
    refresh: refreshMock,
  }),
}));

describe("LoginForm", () => {
  it("shows API error when login fails", async () => {
    // Provide separate responses: 404 for the ALTCHA probe (disabled),
    // then the login failure for the session endpoint.
    const fetchMock = vi.fn().mockImplementation((url: string) => {
      if (typeof url === "string" && url.includes("/api/altcha")) {
        // ALTCHA is not configured – widget hides itself immediately.
        return Promise.resolve({ ok: false, status: 404 });
      }
      return Promise.resolve({
        ok: false,
        json: async () => ({ error: "Invalid credentials" }),
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<LoginForm />);

    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "wrong-password" },
    });

    // Wait for the ALTCHA probe to resolve so the button becomes enabled.
    const button = screen.getByRole("button", { name: "Sign in" });
    await waitFor(() => expect(button).not.toBeDisabled());

    fireEvent.click(button);

    await waitFor(() => {
      expect(screen.getByText("Invalid credentials")).toBeInTheDocument();
    });

    vi.unstubAllGlobals();
  });

  it("re-probes ALTCHA when backend requires payload", async () => {
    let altchaProbeCalls = 0;
    const fetchMock = vi.fn().mockImplementation((url: string) => {
      if (typeof url === "string" && url.includes("/api/altcha")) {
        altchaProbeCalls += 1;
        return Promise.resolve({ ok: false, status: 404 });
      }
      return Promise.resolve({
        ok: false,
        status: 400,
        json: async () => ({
          error: "Bad request: altcha_payload is required",
        }),
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<LoginForm />);

    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "correct-password" },
    });

    const button = screen.getByRole("button", { name: "Sign in" });
    await waitFor(() => expect(button).not.toBeDisabled());

    fireEvent.click(button);

    await waitFor(() => {
      expect(
        screen.getByText("Verification is required. Retrying challenge..."),
      ).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(altchaProbeCalls).toBeGreaterThanOrEqual(4);
    });

    vi.unstubAllGlobals();
  });
});
