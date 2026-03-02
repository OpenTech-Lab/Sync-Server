import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { ACCESS_COOKIE, apiGetJson } from "@/lib/server-api";

export type SessionUser = {
  id: string;
  username: string;
  email: string;
  role: string;
  is_active: boolean;
  created_at: string;
};

export async function requireAdminSession(): Promise<{
  accessToken: string;
  user: SessionUser;
}> {
  const jar = await cookies();
  const accessToken = jar.get(ACCESS_COOKIE)?.value;
  if (!accessToken) {
    redirect("/login");
  }

  let user: SessionUser;
  try {
    user = await apiGetJson<SessionUser>("/auth/me");
  } catch {
    redirect("/login");
  }

  if (user.role !== "admin") {
    redirect("/login");
  }

  return { accessToken, user };
}
