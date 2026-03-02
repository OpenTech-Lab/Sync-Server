import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { ACCESS_COOKIE } from "@/lib/server-api";

import { LoginForm } from "./ui/login-form";

export default async function LoginPage() {
  const jar = await cookies();
  if (jar.get(ACCESS_COOKIE)?.value) {
    redirect("/dashboard");
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-muted/20 p-6">
      <div className="w-full max-w-md rounded-xl border bg-background p-6 shadow-sm">
        <h1 className="text-xl font-semibold">Admin sign in</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Sign in with an admin account to access the Sync dashboard.
        </p>
        <LoginForm />
      </div>
    </div>
  );
}
