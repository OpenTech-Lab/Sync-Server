import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { AuthShell } from "@/app/ui/auth-shell";
import { fetchSetupStatus } from "@/lib/setup-status";
import { ACCESS_COOKIE } from "@/lib/server-api";

import { SetupAdminForm } from "./ui/setup-admin-form";

export default async function SetupAdminPage() {
  const { needsSetup } = await fetchSetupStatus();
  if (!needsSetup) {
    const jar = await cookies();
    if (jar.get(ACCESS_COOKIE)?.value) {
      redirect("/dashboard");
    }
    redirect("/login");
  }

  return (
    <AuthShell
      title="Create admin account"
      description="First-time setup: create the initial admin account for this Sync server."
    >
      <SetupAdminForm />
    </AuthShell>
  );
}
