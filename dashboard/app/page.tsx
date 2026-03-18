import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { fetchSetupStatus } from "@/lib/setup-status";
import { ACCESS_COOKIE, apiGetJson } from "@/lib/server-api";
import type { SessionUser } from "@/lib/session";
import { AuthShell } from "@/app/ui/auth-shell";
import { LoginForm } from "@/app/login/ui/login-form";

export default async function Home() {
  const { needsSetup } = await fetchSetupStatus();
  if (needsSetup) {
    redirect("/setup-admin");
  }

  const jar = await cookies();
  if (jar.get(ACCESS_COOKIE)?.value) {
    try {
      const user = await apiGetJson<SessionUser>("/auth/me");
      if (user.role === "admin") {
        redirect("/dashboard");
      }
    } catch {
      // Ignore invalid/stale token and render login form.
    }
  }

  return (
    <AuthShell
      title="Admin sign in"
      description="Sign in with an admin account to access the Sync dashboard."
    >
      <LoginForm />
    </AuthShell>
  );
}
