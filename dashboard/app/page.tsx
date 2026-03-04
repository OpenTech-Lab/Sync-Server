import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { fetchSetupStatus } from "@/lib/setup-status";
import { ACCESS_COOKIE } from "@/lib/server-api";

export default async function Home() {
  const { needsSetup } = await fetchSetupStatus();
  if (needsSetup) {
    redirect("/login");
  }

  const jar = await cookies();
  if (jar.get(ACCESS_COOKIE)?.value) {
    redirect("/dashboard");
  }
  redirect("/login");
}
