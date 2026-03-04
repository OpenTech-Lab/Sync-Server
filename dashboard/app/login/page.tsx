import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { AuthShell } from "@/app/ui/auth-shell";
import { ACCESS_COOKIE } from "@/lib/server-api";

import { LoginForm } from "./ui/login-form";

export default async function LoginPage() {
  const jar = await cookies();
  if (jar.get(ACCESS_COOKIE)?.value) {
    redirect("/dashboard");
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
