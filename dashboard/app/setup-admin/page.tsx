import { cookies } from "next/headers";
import Image from "next/image";
import { redirect } from "next/navigation";

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
    <div className="flex min-h-screen items-center justify-center bg-muted/20 p-6">
      <div className="w-full max-w-md rounded-xl border bg-background p-6 shadow-sm">
        <div className="mb-4 flex items-center justify-center">
          <Image
            src="/logo.png"
            alt="Sync logo"
            width={56}
            height={56}
            priority
            className="rounded-md"
          />
        </div>
        <h1 className="text-xl font-semibold">Create admin account</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          First-time setup: create the initial admin account for this Sync server.
        </p>
        <SetupAdminForm />
      </div>
    </div>
  );
}
