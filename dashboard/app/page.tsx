import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { ACCESS_COOKIE } from "@/lib/server-api";

export default async function Home() {
  const jar = await cookies();
  if (jar.get(ACCESS_COOKIE)?.value) {
    redirect("/dashboard");
  }
  redirect("/login");
}
