import Link from "next/link";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { AuthShell } from "@/app/ui/auth-shell";
import { fetchSetupStatus } from "@/lib/setup-status";
import { ACCESS_COOKIE } from "@/lib/server-api";

export default async function SetupAdminPage() {
  const { needsSetup } = await fetchSetupStatus();
  if (!needsSetup) {
    const jar = await cookies();
    if (jar.get(ACCESS_COOKIE)?.value) {
      redirect("/dashboard");
    }
    redirect("/");
  }

  return (
    <AuthShell
      title="One-time setup link required"
      description="Open the setup URL printed in the server terminal to create the first admin account."
    >
      <p className="text-sm text-muted-foreground">
        This endpoint requires a one-time token from the startup log.
      </p>
      <p className="mt-4 text-sm">
        <Link className="underline" href="/">
          Back to login
        </Link>
      </p>
    </AuthShell>
  );
}
